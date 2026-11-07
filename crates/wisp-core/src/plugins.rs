//! Расширения на основе манифестов.
//!
//! Плагин — папка `<data_dir>/plugins/<id>/` с файлом `plugin.json`:
//! ```json
//! {
//!   "id": "docker", "name": "Docker", "version": "0.1.0",
//!   "permissions": ["exec"],
//!   "commands": [
//!     { "keyword": "dps", "title": "Docker: контейнеры", "exec": ["docker", "ps"], "output": "text" }
//!   ]
//! }
//! ```
//! Правила безопасности:
//! * плагин по умолчанию выключен; включает его пользователь в настройках;
//! * запуск процессов — только если в манифесте есть `exec` И пользователь это разрешил;
//! * команды запускаются напрямую (argv), без shell — строку запроса нельзя «вклинить» в команду;
//! * всё, что плагин вернул, проверяется: размеры, количество, только http(s)-ссылки.

use crate::commands::run_captured;
use crate::settings::{PluginGrant, Settings};
use crate::{Action, Error, ExecOutcome, ItemKind, Result, SearchItem, matcher, platform};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Запуск процессов.
    Exec,
    /// Открытие ссылок из результатов.
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Показать stdout как текст.
    #[default]
    Text,
    /// stdout — JSON-массив `[{ "title", "subtitle"?, "copy"?, "url"? }]`.
    Items,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommand {
    pub keyword: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    /// argv. Подстановки: `{plugin_dir}` — в любом месте; `{query}` и `{args}` — только целым аргументом.
    pub exec: Vec<String>,
    #[serde(default)]
    pub output: OutputMode,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    pub commands: Vec<PluginCommand>,
}

#[derive(Debug, Clone)]
struct Loaded {
    dir: PathBuf,
    manifest: std::result::Result<Manifest, String>,
    folder: String,
}

/// Описание для экрана настроек.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<Permission>,
    pub commands: Vec<String>,
    pub error: Option<String>,
    pub grant: PluginGrant,
}

#[derive(Debug, Deserialize)]
struct ResultItem {
    title: String,
    #[serde(default)]
    subtitle: String,
    copy: Option<String>,
    url: Option<String>,
}

const MAX_COMMANDS: usize = 50;
const MAX_RESULTS: usize = 100;

fn valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn validate(m: &Manifest, folder: &str) -> std::result::Result<(), String> {
    if !valid_id(&m.id) {
        return Err(format!("некорректный id «{}»: только a-z, 0-9 и «-»", m.id));
    }
    if m.id != folder {
        return Err(format!(
            "id «{}» не совпадает с именем папки «{folder}»",
            m.id
        ));
    }
    if m.commands.is_empty() || m.commands.len() > MAX_COMMANDS {
        return Err(format!("команд должно быть от 1 до {MAX_COMMANDS}"));
    }
    for c in &m.commands {
        if c.keyword.is_empty()
            || c.keyword.len() > 32
            || c.keyword
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_uppercase())
        {
            return Err(format!("некорректное ключевое слово «{}»", c.keyword));
        }
        if c.exec.is_empty() || c.exec[0].trim().is_empty() {
            return Err(format!("у команды «{}» пустой exec", c.keyword));
        }
        if !m.permissions.contains(&Permission::Exec) {
            return Err("команды требуют разрешения \"exec\" в манифесте".into());
        }
    }
    Ok(())
}

pub struct PluginHost {
    dir: PathBuf,
    plugins: RwLock<Vec<Loaded>>,
}

impl PluginHost {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            plugins: RwLock::new(vec![]),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn reload(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
        let mut found = vec![];
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()) {
                let folder = e.file_name().to_string_lossy().into_owned();
                let manifest = std::fs::read_to_string(e.path().join("plugin.json"))
                    .map_err(|err| format!("нет plugin.json: {err}"))
                    .and_then(|t| {
                        serde_json::from_str::<Manifest>(&t)
                            .map_err(|err| format!("ошибка в plugin.json: {err}"))
                    })
                    .and_then(|m| validate(&m, &folder).map(|_| m));
                found.push(Loaded {
                    dir: e.path(),
                    manifest,
                    folder,
                });
            }
        }
        found.sort_by(|a, b| a.folder.cmp(&b.folder));
        log::info!("плагинов: {}", found.len());
        *self.plugins.write().unwrap_or_else(|e| e.into_inner()) = found;
    }

    pub fn list(&self, settings: &Settings) -> Vec<PluginInfo> {
        self.plugins
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|l| {
                let grant = settings.plugins.get(&l.folder).cloned().unwrap_or_default();
                match &l.manifest {
                    Ok(m) => PluginInfo {
                        id: m.id.clone(),
                        name: m.name.clone(),
                        version: m.version.clone(),
                        description: m.description.clone(),
                        permissions: m.permissions.clone(),
                        commands: m.commands.iter().map(|c| c.keyword.clone()).collect(),
                        error: None,
                        grant,
                    },
                    Err(e) => PluginInfo {
                        id: l.folder.clone(),
                        name: l.folder.clone(),
                        version: String::new(),
                        description: String::new(),
                        permissions: vec![],
                        commands: vec![],
                        error: Some(e.clone()),
                        grant,
                    },
                }
            })
            .collect()
    }

    pub fn search(&self, needle: &str, settings: &Settings, limit: usize) -> Vec<SearchItem> {
        let (first, rest) = needle
            .split_once(' ')
            .map_or((needle, ""), |(f, r)| (f, r.trim()));
        let plugins = self.plugins.read().unwrap_or_else(|e| e.into_inner());
        let mut out = vec![];
        for l in plugins.iter() {
            let Ok(m) = &l.manifest else { continue };
            if !settings.plugins.get(&m.id).is_some_and(|g| g.enabled) {
                continue;
            }
            for c in &m.commands {
                let score = if c.keyword == first {
                    110
                } else {
                    matcher::best_score([c.title.as_str(), c.keyword.as_str()], needle)
                };
                if score == 0 {
                    continue;
                }
                let subtitle = match (
                    c.keyword == first && !rest.is_empty(),
                    c.subtitle.is_empty(),
                ) {
                    (true, _) => format!("{} · {rest}", m.name),
                    (false, true) => m.name.clone(),
                    (false, false) => c.subtitle.clone(),
                };
                out.push(SearchItem {
                    id: format!("plugin:{}:{}", m.id, c.keyword),
                    kind: ItemKind::Plugin,
                    title: c.title.clone(),
                    subtitle,
                    danger: false,
                    score,
                    action: Action::Plugin {
                        plugin: m.id.clone(),
                        keyword: c.keyword.clone(),
                    },
                });
            }
        }
        out.sort_by_key(|i| std::cmp::Reverse(i.score));
        out.truncate(limit);
        out
    }

    /// `query` — исходный запрос пользователя (с регистром); аргументами считается всё после ключевого слова.
    pub async fn run(
        &self,
        plugin: &str,
        keyword: &str,
        query: &str,
        settings: &Settings,
    ) -> Result<(ExecOutcome, Vec<SearchItem>)> {
        let (dir, m, cmd) = {
            let plugins = self.plugins.read().unwrap_or_else(|e| e.into_inner());
            let l = plugins
                .iter()
                .find(|l| l.folder == plugin)
                .ok_or(Error::UnknownItem)?;
            let m = l.manifest.as_ref().map_err(|e| Error::Other(e.clone()))?;
            let cmd = m
                .commands
                .iter()
                .find(|c| c.keyword == keyword)
                .ok_or(Error::UnknownItem)?;
            (l.dir.clone(), m.clone(), cmd.clone())
        };
        let grant = settings.plugins.get(plugin).cloned().unwrap_or_default();
        if !grant.enabled {
            return Err(Error::Forbidden(format!("плагин «{}» выключен", m.name)));
        }
        if !grant.allow_exec {
            return Err(Error::Forbidden(format!(
                "плагину «{}» не разрешено запускать программы — включите в настройках",
                m.name
            )));
        }

        let query = query.trim();
        let args_str = query
            .split_once(char::is_whitespace)
            .filter(|(first, _)| first.eq_ignore_ascii_case(keyword))
            .map_or("", |(_, rest)| rest.trim());
        let args = platform::split_args(Some(args_str));
        let dir_str = dir.to_string_lossy();

        let mut argv: Vec<String> = vec![];
        for a in &cmd.exec {
            match a.as_str() {
                "{args}" => argv.extend(args.iter().cloned()),
                "{query}" => argv.push(args_str.to_string()),
                other => argv.push(other.replace("{plugin_dir}", &dir_str)),
            }
        }
        let mut proc = platform::program_command(&argv[0]);
        proc.args(&argv[1..])
            .current_dir(&dir)
            .env("WISP_QUERY", args_str);
        let timeout = Duration::from_secs(cmd.timeout_secs.unwrap_or(15).clamp(1, 60));
        let out = run_captured(proc, timeout).await?;

        match cmd.output {
            OutputMode::Text => Ok((
                ExecOutcome::Output {
                    title: cmd.title.clone(),
                    stdout: out.stdout,
                    stderr: out.stderr,
                    code: out.code,
                    timed_out: out.timed_out,
                },
                vec![],
            )),
            OutputMode::Items => {
                if out.timed_out {
                    return Err(Error::Other(format!(
                        "плагин «{}» не ответил за {} с",
                        m.name,
                        timeout.as_secs()
                    )));
                }
                let items = parse_items(&m, &out.stdout, grant.allow_open)?;
                Ok((
                    ExecOutcome::Items {
                        title: cmd.title.clone(),
                        items: vec![],
                    },
                    items,
                ))
            }
        }
    }
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Проверка ответа плагина. Всё лишнее молча отбрасывается.
fn parse_items(m: &Manifest, stdout: &str, allow_open: bool) -> Result<Vec<SearchItem>> {
    let raw: Vec<ResultItem> = serde_json::from_str(stdout.trim())
        .map_err(|e| Error::Other(format!("плагин «{}» вернул некорректный JSON: {e}", m.name)))?;
    let can_open = allow_open && m.permissions.contains(&Permission::Open);
    Ok(raw
        .into_iter()
        .take(MAX_RESULTS)
        .enumerate()
        .filter_map(|(i, r)| {
            let title = clip(&r.title, 200);
            if title.is_empty() {
                return None;
            }
            let action = match (r.copy, r.url) {
                (_, Some(url))
                    if can_open
                        && (url.starts_with("https://") || url.starts_with("http://"))
                        && url.len() <= 2048 =>
                {
                    Action::OpenUrl { url }
                }
                (Some(text), _) if text.len() <= 10_000 => Action::CopyText { text },
                _ => Action::CopyText {
                    text: title.clone(),
                },
            };
            Some(SearchItem {
                id: format!("pres:{}:{i}", m.id),
                kind: ItemKind::Plugin,
                title,
                subtitle: clip(&r.subtitle, 300),
                danger: false,
                score: 0,
                action,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).unwrap()
    }

    const OK: &str = r#"{"id":"demo","name":"Demo","version":"1","permissions":["exec","open"],
        "commands":[{"keyword":"hi","title":"Скажи привет","exec":["echo","{args}"]}]}"#;

    #[test]
    fn manifest_validation() {
        assert!(validate(&manifest(OK), "demo").is_ok());
        assert!(validate(&manifest(OK), "other").is_err());
        let no_exec = OK.replace(r#""exec","open""#, r#""open""#);
        assert!(validate(&manifest(&no_exec), "demo").is_err());
        let bad_kw = OK.replace(r#""keyword":"hi""#, r#""keyword":"Hi there""#);
        assert!(validate(&manifest(&bad_kw), "demo").is_err());
    }

    #[test]
    fn result_items_are_sanitized() {
        let m = manifest(OK);
        let out = r#"[{"title":"a","url":"https://ok"},{"title":"b","url":"file:///etc/passwd"},{"title":"  "},{"title":"c","copy":"x"}]"#;
        let items = parse_items(&m, out, true).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0].action,
            Action::OpenUrl {
                url: "https://ok".into()
            }
        );
        assert_eq!(
            items[1].action,
            Action::CopyText { text: "b".into() },
            "file: не открываем"
        );
        // без разрешения пользователя ссылки не открываются
        let items = parse_items(&m, out, false).unwrap();
        assert!(matches!(items[0].action, Action::CopyText { .. }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_only_when_granted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("demo")).unwrap();
        std::fs::write(dir.path().join("demo/plugin.json"), OK).unwrap();
        let host = PluginHost::new(dir.path().to_path_buf());
        host.reload();

        let mut s = Settings::default();
        assert!(host.search("hi", &s, 5).is_empty(), "выключен по умолчанию");
        s.plugins.insert(
            "demo".into(),
            PluginGrant {
                enabled: true,
                allow_exec: false,
                allow_open: false,
            },
        );
        assert_eq!(host.search("hi", &s, 5).len(), 1);
        assert!(
            host.run("demo", "hi", "hi мир", &s).await.is_err(),
            "нет разрешения exec"
        );

        s.plugins.get_mut("demo").unwrap().allow_exec = true;
        // «; rm -rf» уходит отдельным аргументом echo, а не в shell
        let (out, _) = host
            .run("demo", "hi", "hi мир; rm -rf x", &s)
            .await
            .unwrap();
        match out {
            ExecOutcome::Output { stdout, .. } => assert_eq!(stdout.trim(), "мир; rm -rf x"),
            other => panic!("{other:?}"),
        }
    }
}
