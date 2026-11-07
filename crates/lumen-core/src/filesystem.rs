//! Индекс файлов: обход выбранных папок + отслеживание изменений через `notify`.
//!
//! Почему не Tantivy / FTS5: мы ищем по именам файлов, а не по содержимому.
//! Для этого быстрее и проще держать в памяти плоский список имён в нижнем регистре
//! (≈ 100 байт на файл, 300 тыс. файлов ≈ 30 МБ) и прогонять по нему нечёткий поиск
//! с проверкой отмены. Полнотекстовый поиск по содержимому — кандидат для FTS5 на следующих этапах.

use crate::matcher;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

/// Верхняя граница, чтобы случайно добавленный `C:\` не съел всю память.
pub const MAX_ENTRIES: usize = 300_000;
const MAX_DEPTH: usize = 10;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    lower: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

impl FileEntry {
    fn new(path: &Path, is_dir: bool) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        Some(Self {
            lower: name.to_lowercase(),
            name,
            path: path.to_path_buf(),
            is_dir,
        })
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub indexing: bool,
    pub files: usize,
    pub truncated: bool,
}

#[derive(Default)]
struct Config {
    roots: Vec<PathBuf>,
    excludes: Vec<String>,
}

#[derive(Default)]
pub struct FileIndex {
    entries: RwLock<Vec<FileEntry>>,
    config: RwLock<Config>,
    status: RwLock<IndexStatus>,
    watcher: Mutex<Option<Debouncer<RecommendedWatcher>>>,
}

/// Исключения: точное имя («node_modules») или маска расширения («*.tmp»). Скрытые (`.git`, `.cache`) — всегда.
pub fn is_excluded(name: &str, excludes: &[String]) -> bool {
    if name.starts_with('.') {
        return true;
    }
    let lower = name.to_lowercase();
    excludes.iter().any(|p| match p.strip_prefix("*.") {
        Some(ext) => lower.ends_with(&format!(".{}", ext.to_lowercase())),
        None => lower == p.to_lowercase(),
    })
}

fn walk(root: &Path, excludes: &[String], out: &mut Vec<FileEntry>) -> bool {
    let walker = walkdir::WalkDir::new(root)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.depth() == 0 || !is_excluded(&e.file_name().to_string_lossy(), excludes)
        });
    for e in walker.filter_map(|e| e.ok()) {
        if e.depth() == 0 {
            continue;
        }
        if out.len() >= MAX_ENTRIES {
            return false;
        }
        if let Some(f) = FileEntry::new(e.path(), e.file_type().is_dir()) {
            out.push(f);
        }
    }
    true
}

impl FileIndex {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn status(&self) -> IndexStatus {
        self.status
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Полная переиндексация. Блокирующая — вызывать из фонового потока.
    pub fn rebuild(self: &Arc<Self>, roots: Vec<PathBuf>, excludes: Vec<String>) {
        self.status
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .indexing = true;
        let started = std::time::Instant::now();

        let mut fresh = Vec::new();
        let mut complete = true;
        for root in roots.iter().filter(|r| r.is_dir()) {
            complete &= walk(root, &excludes, &mut fresh);
        }
        let count = fresh.len();
        *self.entries.write().unwrap_or_else(|e| e.into_inner()) = fresh;
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = Config {
            roots: roots.clone(),
            excludes,
        };
        *self.status.write().unwrap_or_else(|e| e.into_inner()) = IndexStatus {
            indexing: false,
            files: count,
            truncated: !complete,
        };
        log::info!(
            "проиндексировано файлов: {count} за {:?}",
            started.elapsed()
        );

        self.watch(&roots);
    }

    fn watch(self: &Arc<Self>, roots: &[PathBuf]) {
        let weak: Weak<Self> = Arc::downgrade(self);
        let debouncer = new_debouncer(
            Duration::from_millis(800),
            move |res: DebounceEventResult| {
                if let (Ok(events), Some(index)) = (res, weak.upgrade()) {
                    index.apply_changes(events.into_iter().map(|e| e.path).collect());
                }
            },
        );
        let mut debouncer = match debouncer {
            Ok(d) => d,
            Err(e) => {
                log::warn!("не удалось запустить отслеживание файлов: {e}");
                return;
            }
        };
        for root in roots.iter().filter(|r| r.is_dir()) {
            if let Err(e) = debouncer.watcher().watch(root, RecursiveMode::Recursive) {
                log::warn!("не удалось следить за {}: {e}", root.display());
            }
        }
        // Старый наблюдатель останавливается при замене.
        *self.watcher.lock().unwrap_or_else(|e| e.into_inner()) = Some(debouncer);
    }

    /// Точечное обновление по событиям файловой системы.
    pub fn apply_changes(self: &Arc<Self>, paths: Vec<PathBuf>) {
        let (roots, excludes) = {
            let c = self.config.read().unwrap_or_else(|e| e.into_inner());
            (c.roots.clone(), c.excludes.clone())
        };
        let relevant: Vec<PathBuf> = paths
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .filter(|p| {
                roots.iter().find(|r| p.starts_with(r)).is_some_and(|root| {
                    p.strip_prefix(root)
                        .map(|rel| {
                            !rel.components()
                                .any(|c| is_excluded(&c.as_os_str().to_string_lossy(), &excludes))
                        })
                        .unwrap_or(false)
                })
            })
            .collect();
        if relevant.is_empty() {
            return;
        }
        if relevant.len() > 500 {
            // Массовые изменения (распаковали архив) — проще пересобрать.
            let me = Arc::clone(self);
            std::thread::spawn(move || me.rebuild(roots, excludes));
            return;
        }

        let mut added = Vec::new();
        for p in &relevant {
            if let Ok(meta) = std::fs::symlink_metadata(p) {
                if let Some(f) = FileEntry::new(p, meta.is_dir()) {
                    added.push(f);
                }
                if meta.is_dir() {
                    walk(p, &excludes, &mut added);
                }
            }
        }

        let mut entries = self.entries.write().unwrap_or_else(|e| e.into_inner());
        entries.retain(|e| !relevant.iter().any(|p| e.path.starts_with(p)));
        let room = MAX_ENTRIES.saturating_sub(entries.len());
        entries.extend(added.into_iter().take(room));
        let n = entries.len();
        drop(entries);
        self.status.write().unwrap_or_else(|e| e.into_inner()).files = n;
    }

    /// `needle` — в нижнем регистре. `ext` — фильтр вида ".rs".
    /// `cancelled` проверяется по ходу: устаревший запрос прекращается досрочно.
    pub fn search(
        &self,
        needle: &str,
        ext: Option<&str>,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<FileEntry>> {
        let entries = self.entries.read().unwrap_or_else(|e| e.into_inner());
        let mut hits: Vec<(u32, usize)> = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            if i % 8192 == 0 && cancelled() {
                return None;
            }
            if let Some(ext) = ext {
                if !e.is_dir
                    && e.lower.ends_with(ext)
                    && (needle.is_empty() || e.lower.contains(needle))
                {
                    hits.push((50, i));
                }
                continue;
            }
            let s = matcher::score_lowered(&e.lower, needle);
            // Подпоследовательность по огромному списку даёт мусор — для файлов её не берём.
            if s > 20 {
                hits.push((s, i));
            }
        }
        let by_rank = |a: &(u32, usize), b: &(u32, usize)| {
            b.0.cmp(&a.0)
                .then_with(|| entries[a.1].name.len().cmp(&entries[b.1].name.len()))
        };
        if hits.len() > limit {
            hits.select_nth_unstable_by(limit, by_rank);
            hits.truncate(limit);
        }
        hits.sort_by(by_rank);
        Some(hits.into_iter().map(|(_, i)| entries[i].clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn index_search_and_watch_updates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("src/main.rs"), "").unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        fs::write(root.join("node_modules/pkg/main.rs"), "").unwrap();
        fs::write(root.join("junk.tmp"), "").unwrap();

        let idx = FileIndex::new();
        let excludes = vec!["node_modules".to_string(), "*.tmp".to_string()];
        idx.rebuild(vec![root.clone()], excludes);
        let never = || false;

        let r = idx.search("main", None, 10, &never).unwrap();
        assert_eq!(r.len(), 1, "node_modules исключён");
        assert!(
            idx.search("junk", None, 10, &never).unwrap().is_empty(),
            "*.tmp исключён"
        );
        assert_eq!(
            idx.search("", Some(".md"), 10, &never).unwrap()[0].name,
            "README.md"
        );
        assert!(idx.search("main", None, 10, &|| true).is_none(), "отмена");

        // точечные изменения
        fs::write(root.join("src/lib.rs"), "").unwrap();
        idx.apply_changes(vec![root.join("src/lib.rs")]);
        assert_eq!(idx.search("lib.rs", None, 10, &never).unwrap().len(), 1);
        fs::remove_dir_all(root.join("src")).unwrap();
        idx.apply_changes(vec![root.join("src")]);
        assert!(idx.search("main", None, 10, &never).unwrap().is_empty());
    }
}
