//! Центральная логика: разбор запроса, опрос провайдеров, кэш результатов, выполнение действий.

use crate::calculator;
use crate::commands::{self, run_captured};
use crate::filesystem::{FileIndex, IndexStatus};
use crate::launcher::AppIndex;
use crate::platform;
use crate::plugins::{PluginHost, PluginInfo};
use crate::settings::Settings;
use crate::storage::Storage;
use crate::{
    Action, CalcView, Error, ExecOutcome, ItemKind, ItemView, Result, SearchItem, SearchResponse,
    SectionView,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

const CACHE_LIMIT: usize = 5_000;
const SHELL_TIMEOUT: Duration = Duration::from_secs(30);

/// Разделитель запроса и аргументов запуска: `code -- C:\Projects\wisp`.
pub const ARGS_SEPARATOR: &str = " -- ";

pub fn split_query(raw: &str) -> (&str, Option<&str>) {
    match raw.split_once(ARGS_SEPARATOR) {
        Some((q, a)) => (q.trim(), Some(a.trim()).filter(|a| !a.is_empty())),
        None => (raw.trim(), None),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub apps: usize,
    pub files: IndexStatus,
    pub plugins_dir: String,
}

pub struct Engine {
    storage: Storage,
    settings: RwLock<Settings>,
    apps: AppIndex,
    files: Arc<FileIndex>,
    plugins: PluginHost,
    latest_request: AtomicU64,
    /// Результаты последних запросов: UI присылает только id, действие берётся отсюда.
    cache: Mutex<HashMap<String, SearchItem>>,
}

impl Engine {
    /// Открывает БД в `data_dir/wisp.db`, плагины ищет в `data_dir/plugins`.
    pub fn open(data_dir: &Path) -> Result<Arc<Self>> {
        let storage = Storage::open(&data_dir.join("wisp.db"))?;
        Ok(Self::with_storage(storage, data_dir.join("plugins")))
    }

    pub fn with_storage(storage: Storage, plugins_dir: std::path::PathBuf) -> Arc<Self> {
        let settings = storage.load_settings().unwrap_or_default();
        Arc::new(Self {
            storage,
            settings: RwLock::new(settings),
            apps: AppIndex::default(),
            files: FileIndex::new(),
            plugins: PluginHost::new(plugins_dir),
            latest_request: AtomicU64::new(0),
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Индексация в фоне: окно доступно сразу, результаты появляются по мере готовности.
    pub fn start_background(self: &Arc<Self>) {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("wisp-index".into())
            .spawn(move || {
                me.plugins.reload();
                me.apps.refresh();
                let s = me.settings();
                me.files.rebuild(s.index_roots, s.index_excludes);
            })
            .expect("не удалось запустить поток индексации");
    }

    pub fn reindex(self: &Arc<Self>) {
        self.start_background();
    }

    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Сохраняет настройки; при смене папок индексации — переиндексирует.
    pub fn update_settings(self: &Arc<Self>, new: Settings) -> Result<Settings> {
        let new = new.sanitized();
        self.storage.save_settings(&new)?;
        let old = std::mem::replace(
            &mut *self.settings.write().unwrap_or_else(|e| e.into_inner()),
            new.clone(),
        );
        if old.index_roots != new.index_roots || old.index_excludes != new.index_excludes {
            let files = Arc::clone(&self.files);
            let (roots, ex) = (new.index_roots.clone(), new.index_excludes.clone());
            std::thread::spawn(move || files.rebuild(roots, ex));
        }
        Ok(new)
    }

    pub fn status(&self) -> Status {
        Status {
            apps: self.apps.len(),
            files: self.files.status(),
            plugins_dir: self.plugins.dir().display().to_string(),
        }
    }

    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins.list(&self.settings())
    }

    pub fn reload_plugins(&self) {
        self.plugins.reload();
    }

    pub fn clear_history(&self) -> Result<()> {
        self.storage.clear_history()
    }

    // ------------------------------------------------------------------ поиск

    /// Синхронный — вызывать из `spawn_blocking`. Новый `request_id` отменяет все предыдущие запросы.
    pub fn search(&self, raw: &str, request_id: u64) -> SearchResponse {
        // Запросы могут прийти не по порядку: более старый сразу считается отменённым.
        if self.latest_request.fetch_max(request_id, Ordering::SeqCst) > request_id {
            return SearchResponse::cancelled(request_id);
        }
        let cancelled = || self.latest_request.load(Ordering::SeqCst) != request_id;

        let settings = self.settings();
        let limit = settings.max_results_per_section;
        let raw = raw.trim();
        let mut sections: Vec<(String, Vec<SearchItem>)> = vec![];
        let mut calc: Option<(SearchItem, CalcView)> = None;

        if raw.is_empty() {
            let recent = self.storage.recent(5).unwrap_or_default();
            let favorites = self.storage.favorites().unwrap_or_default();
            let shown: HashSet<String> = recent
                .iter()
                .chain(&favorites)
                .map(|i| i.id.clone())
                .collect();
            let suggestions = commands::suggestions()
                .into_iter()
                .filter(|i| !shown.contains(&i.id))
                .collect();
            sections.push(("Недавние".into(), recent));
            sections.push(("Избранное".into(), favorites));
            sections.push(("Предложения".into(), suggestions));
        } else if let Some(cmd) = raw.strip_prefix('>') {
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                sections.push((
                    format!("Выполнить в {}", platform::SHELL_NAME),
                    vec![commands::shell_item(cmd)],
                ));
            }
        } else {
            let (query, _args) = split_query(raw);
            let needle = query.to_lowercase();

            if needle.starts_with('.') && needle.len() > 1 && !needle.contains(' ') {
                // «.pdf» — все файлы с расширением
                if settings.providers.files {
                    match self.files.search("", Some(&needle), limit * 3, &cancelled) {
                        Some(found) => sections.push((
                            format!("Файлы {needle}"),
                            found.iter().map(file_item).collect(),
                        )),
                        None => return SearchResponse::cancelled(request_id),
                    }
                }
            } else {
                if settings.providers.calculator
                    && let Some(r) = calculator::evaluate(query)
                {
                    let item = SearchItem {
                        id: "calc".into(),
                        kind: ItemKind::Calc,
                        title: r.display.clone(),
                        subtitle: query.into(),
                        danger: false,
                        score: 1000,
                        action: Action::CopyText {
                            text: r.copy.clone(),
                        },
                    };
                    let view = CalcView {
                        id: item.id.clone(),
                        expression: query.into(),
                        display: r.display,
                        words: "Ответ · ↵ скопировать".into(),
                    };
                    calc = Some((item, view));
                }
                if settings.providers.apps {
                    let counts = self.storage.launch_counts().unwrap_or_default();
                    sections.push((
                        "Приложения".into(),
                        self.apps.search(&needle, limit, &counts),
                    ));
                }
                if settings.providers.commands {
                    sections.push((
                        "Команды".into(),
                        commands::search(&needle, &settings, limit),
                    ));
                }
                if settings.providers.plugins {
                    sections.push((
                        "Расширения".into(),
                        self.plugins.search(&needle, &settings, limit),
                    ));
                }
                if cancelled() {
                    return SearchResponse::cancelled(request_id);
                }
                // Файлы — самое дорогое, поэтому последними и только от 2 символов.
                if settings.providers.files && needle.chars().count() >= 2 {
                    match self.files.search(&needle, None, limit, &cancelled) {
                        Some(found) => {
                            sections.push(("Файлы".into(), found.iter().map(file_item).collect()))
                        }
                        None => return SearchResponse::cancelled(request_id),
                    }
                }
            }
        }

        if cancelled() {
            return SearchResponse::cancelled(request_id);
        }

        let favorites = self.storage.favorite_ids().unwrap_or_default();
        {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() > CACHE_LIMIT {
                cache.clear();
            }
            for item in sections
                .iter()
                .flat_map(|(_, items)| items)
                .chain(calc.as_ref().map(|(i, _)| i))
            {
                cache.insert(item.id.clone(), item.clone());
            }
        }

        SearchResponse {
            request_id,
            cancelled: false,
            calc: calc.map(|(_, v)| v),
            sections: sections
                .into_iter()
                .filter(|(_, items)| !items.is_empty())
                .map(|(title, items)| SectionView {
                    title,
                    items: items
                        .iter()
                        .map(|i| i.view(favorites.contains(&i.id)))
                        .collect(),
                })
                .collect(),
        }
    }

    fn lookup(&self, id: &str) -> Result<SearchItem> {
        if let Some(i) = self.cache.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
            return Ok(i.clone());
        }
        self.storage
            .favorites()?
            .into_iter()
            .chain(self.storage.recent(50)?)
            .find(|i| i.id == id)
            .ok_or(Error::UnknownItem)
    }

    /// «Показать в папке» (Ctrl+Enter) для файлов, папок и приложений с путём.
    pub fn reveal(&self, id: &str) -> Result<()> {
        let item = self.lookup(id)?;
        let path = match &item.action {
            Action::OpenPath { path } => path.clone(),
            Action::LaunchApp { target } if Path::new(target).exists() => target.into(),
            _ => {
                return Err(Error::Other(
                    "у этого результата нет расположения на диске".into(),
                ));
            }
        };
        platform::reveal(&path)
    }

    pub fn toggle_favorite(&self, id: &str) -> Result<bool> {
        let item = self.lookup(id)?;
        if matches!(item.kind, ItemKind::Calc | ItemKind::Shell) || id.starts_with("pres:") {
            return Err(Error::Other(
                "этот результат нельзя добавить в избранное".into(),
            ));
        }
        self.storage.toggle_favorite(&item)
    }

    // ------------------------------------------------------------------ выполнение

    /// `query` — текущий текст в поиске (нужен для аргументов). Опасные действия без `confirmed`
    /// не выполняются — проверка здесь, а не только в UI.
    pub async fn execute(&self, id: &str, confirmed: bool, query: &str) -> Result<ExecOutcome> {
        let item = self.lookup(id)?;
        let settings = self.settings();

        let danger = match &item.action {
            Action::Shell { command } => commands::is_dangerous_shell(command),
            Action::System { action } => action.is_dangerous(),
            _ => item.danger,
        };
        if danger && settings.confirm_dangerous && !confirmed {
            return Ok(ExecOutcome::NeedsConfirmation {
                id: item.id.clone(),
                title: item.title.clone(),
            });
        }

        let (_, args) = split_query(query);
        let done = |verb: &str| ExecOutcome::Done {
            message: format!("{verb}: {}", item.title),
            hide: true,
        };

        let outcome = match &item.action {
            Action::LaunchApp { target } => {
                platform::launch_app(target, args)?;
                done("Открыто")
            }
            Action::OpenPath { path } => {
                platform::open_path(path)?;
                done("Открыто")
            }
            Action::OpenUrl { url } => {
                platform::open_url(url)?;
                done("Открыто")
            }
            Action::System { action } => {
                platform::system_action(*action)?;
                done("Выполнено")
            }
            Action::CopyText { text } => ExecOutcome::Copy {
                text: text.clone(),
                message: format!("Скопировано: {}", item.title),
            },
            Action::Internal { action } => ExecOutcome::Internal { action: *action },
            Action::Shell { command } => self.run_shell(&item.title, command, &settings).await?,
            Action::UserCommand { command, shell } => {
                let alias_args = query
                    .trim()
                    .split_once(char::is_whitespace)
                    .map(|(_, rest)| rest.trim())
                    .filter(|r| !r.is_empty());
                let expanded = commands::expand_alias(command, alias_args);
                if *shell {
                    self.run_shell(&item.title, &expanded, &settings).await?
                } else if expanded.starts_with("http://") || expanded.starts_with("https://") {
                    platform::open_url(&expanded)?;
                    done("Открыто")
                } else {
                    let p = Path::new(expanded.trim());
                    if p.is_dir()
                        || p.extension()
                            .is_some_and(|e| !e.eq_ignore_ascii_case("exe"))
                    {
                        platform::open_path(p)?;
                    } else {
                        platform::launch_app(expanded.trim(), args)?;
                    }
                    done("Открыто")
                }
            }
            Action::Plugin { plugin, keyword } => {
                let (outcome, results) =
                    self.plugins.run(plugin, keyword, query, &settings).await?;
                match outcome {
                    ExecOutcome::Items { title, .. } => {
                        let views: Vec<ItemView> = results.iter().map(|r| r.view(false)).collect();
                        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
                        for r in results {
                            cache.insert(r.id.clone(), r);
                        }
                        ExecOutcome::Items {
                            title,
                            items: views,
                        }
                    }
                    other => other,
                }
            }
        };

        let worth_remembering =
            !matches!(item.kind, ItemKind::Calc) && !item.id.starts_with("pres:");
        if settings.record_history
            && worth_remembering
            && let Err(e) = self.storage.record_launch(&item)
        {
            log::warn!("история не записана: {e}");
        }
        Ok(outcome)
    }

    async fn run_shell(
        &self,
        title: &str,
        command: &str,
        settings: &Settings,
    ) -> Result<ExecOutcome> {
        let out = run_captured(platform::shell_command(command), SHELL_TIMEOUT).await?;
        if !settings.show_command_output {
            let ok = out.code == Some(0);
            return Ok(ExecOutcome::Done {
                message: if ok {
                    format!("Готово: {title}")
                } else {
                    format!("Ошибка ({:?}): {title}", out.code)
                },
                hide: ok,
            });
        }
        Ok(ExecOutcome::Output {
            title: title.into(),
            stdout: out.stdout,
            stderr: out.stderr,
            code: out.code,
            timed_out: out.timed_out,
        })
    }
}

fn file_item(f: &crate::filesystem::FileEntry) -> SearchItem {
    let parent = f
        .path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    SearchItem {
        id: format!("file:{}", f.path.display()),
        kind: if f.is_dir {
            ItemKind::Folder
        } else {
            ItemKind::File
        },
        title: f.name.clone(),
        subtitle: parent,
        danger: false,
        score: 0,
        action: Action::OpenPath {
            path: f.path.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::AppEntry;

    fn engine() -> Arc<Engine> {
        let dir = tempfile::tempdir().unwrap();
        let e = Engine::with_storage(Storage::in_memory().unwrap(), dir.keep().join("plugins"));
        e.apps.set(vec![AppEntry {
            title: "Visual Studio Code".into(),
            subtitle: String::new(),
            target: "code".into(),
        }]);
        e
    }

    #[test]
    fn query_split() {
        assert_eq!(split_query("code -- C:\\p"), ("code", Some("C:\\p")));
        assert_eq!(split_query(" tel "), ("tel", None));
    }

    #[tokio::test]
    async fn search_then_execute_flow() {
        let e = engine();
        let r = e.search("vsc", 1);
        assert_eq!(r.sections[0].items[0].title, "Visual Studio Code");

        // калькулятор → копирование
        let r = e.search("2+2*2", 2);
        let calc = r.calc.unwrap();
        assert_eq!(calc.display, "6");
        match e.execute(&calc.id, false, "2+2*2").await.unwrap() {
            ExecOutcome::Copy { text, .. } => assert_eq!(text, "6"),
            o => panic!("{o:?}"),
        }

        // опасная команда без подтверждения не выполняется
        let r = e.search("выключить", 3);
        let id = &r
            .sections
            .iter()
            .find(|s| s.title == "Команды")
            .unwrap()
            .items[0]
            .id;
        assert!(matches!(
            e.execute(id, false, "выключить").await.unwrap(),
            ExecOutcome::NeedsConfirmation { .. }
        ));

        // id, которого не было в выдаче, выполнить нельзя
        assert!(e.execute("shell:rm -rf /", true, "").await.is_err());
    }

    #[tokio::test]
    async fn shell_output_and_danger() {
        let e = engine();
        let r = e.search("> echo wisp", 1);
        let id = r.sections[0].items[0].id.clone();
        match e.execute(&id, false, "> echo wisp").await.unwrap() {
            ExecOutcome::Output { stdout, code, .. } => {
                assert_eq!(stdout.trim(), "wisp");
                assert_eq!(code, Some(0));
            }
            o => panic!("{o:?}"),
        }
        let r = e.search("> rm -rf something", 2);
        let item = &r.sections[0].items[0];
        assert!(item.danger);
        assert!(matches!(
            e.execute(&item.id, false, "").await.unwrap(),
            ExecOutcome::NeedsConfirmation { .. }
        ));
    }

    #[test]
    fn out_of_order_request_is_cancelled() {
        let e = engine();
        assert!(!e.search("vsc", 10).cancelled);
        assert!(
            e.search("vs", 9).cancelled,
            "ответ на старый запрос не нужен"
        );
    }

    #[test]
    fn empty_query_and_favorites() {
        let e = engine();
        let r = e.search("vsc", 1);
        let id = r.sections[0].items[0].id.clone();
        assert!(e.toggle_favorite(&id).unwrap());
        let r = e.search("", 2);
        let fav = r.sections.iter().find(|s| s.title == "Избранное").unwrap();
        assert!(fav.items[0].favorite);
    }
}
