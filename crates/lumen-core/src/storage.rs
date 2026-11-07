//! SQLite: история запусков, избранное, настройки.
//!
//! Секреты (пароли, токены) здесь не хранятся вообще. Когда они понадобятся плагинам,
//! их место — системное хранилище (Windows Credential Manager / macOS Keychain через crate `keyring`).

use crate::{Result, SearchItem, Settings};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Storage {
    conn: Mutex<Connection>,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS history (
    id         TEXT PRIMARY KEY,
    item_json  TEXT    NOT NULL,
    count      INTEGER NOT NULL DEFAULT 0,
    last_used  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS favorites (
    id        TEXT PRIMARY KEY,
    item_json TEXT    NOT NULL,
    added_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        // Паника в другом потоке не должна навсегда ломать БД.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---------- настройки ----------

    pub fn load_settings(&self) -> Result<Settings> {
        let raw: Option<String> = self
            .db()
            .query_row("SELECT value FROM kv WHERE key = 'settings'", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(match raw {
            // Неизвестные/битые поля не роняют приложение: берутся значения по умолчанию.
            Some(json) => serde_json::from_str::<Settings>(&json)
                .unwrap_or_default()
                .sanitized(),
            None => Settings::default(),
        })
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(s)?;
        self.db().execute(
            "INSERT INTO kv(key, value) VALUES('settings', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    // ---------- история ----------

    pub fn record_launch(&self, item: &SearchItem) -> Result<()> {
        let json = serde_json::to_string(item)?;
        self.db().execute(
            "INSERT INTO history(id, item_json, count, last_used) VALUES(?1, ?2, 1, ?3)
             ON CONFLICT(id) DO UPDATE SET item_json = excluded.item_json, count = count + 1, last_used = excluded.last_used",
            params![item.id, json, now()],
        )?;
        Ok(())
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<SearchItem>> {
        self.items(
            "SELECT item_json FROM history ORDER BY last_used DESC LIMIT ?1",
            limit,
        )
    }

    /// Сколько раз запускали каждый id — для поднятия часто используемого в выдаче.
    pub fn launch_counts(&self) -> Result<std::collections::HashMap<String, u32>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, count FROM history")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn clear_history(&self) -> Result<()> {
        self.db().execute("DELETE FROM history", [])?;
        Ok(())
    }

    // ---------- избранное ----------

    /// Возвращает новое состояние: `true` — теперь в избранном.
    pub fn toggle_favorite(&self, item: &SearchItem) -> Result<bool> {
        let db = self.db();
        let removed = db.execute("DELETE FROM favorites WHERE id = ?1", params![item.id])?;
        if removed > 0 {
            return Ok(false);
        }
        db.execute(
            "INSERT INTO favorites(id, item_json, added_at) VALUES(?1, ?2, ?3)",
            params![item.id, serde_json::to_string(item)?, now()],
        )?;
        Ok(true)
    }

    pub fn favorites(&self) -> Result<Vec<SearchItem>> {
        self.items(
            "SELECT item_json FROM favorites ORDER BY added_at LIMIT ?1",
            50,
        )
    }

    pub fn favorite_ids(&self) -> Result<std::collections::HashSet<String>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id FROM favorites")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn items(&self, sql: &str, limit: usize) -> Result<Vec<SearchItem>> {
        let db = self.db();
        let mut st = db.prepare(sql)?;
        let rows = st.query_map(params![limit as i64], |r| r.get::<_, String>(0))?;
        Ok(rows
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ItemKind};

    fn item(id: &str) -> SearchItem {
        SearchItem {
            id: id.into(),
            kind: ItemKind::App,
            title: id.into(),
            subtitle: String::new(),
            danger: false,
            score: 0,
            action: Action::LaunchApp { target: id.into() },
        }
    }

    #[test]
    fn history_and_favorites() {
        let s = Storage::in_memory().unwrap();
        s.record_launch(&item("a")).unwrap();
        s.record_launch(&item("a")).unwrap();
        s.record_launch(&item("b")).unwrap();
        assert_eq!(s.launch_counts().unwrap()["a"], 2);
        assert_eq!(s.recent(10).unwrap().len(), 2);

        assert!(s.toggle_favorite(&item("a")).unwrap());
        assert_eq!(s.favorites().unwrap().len(), 1);
        assert!(!s.toggle_favorite(&item("a")).unwrap());
        assert!(s.favorites().unwrap().is_empty());
    }

    #[test]
    fn settings_roundtrip() {
        let s = Storage::in_memory().unwrap();
        let st = Settings {
            hotkey: "Ctrl+Space".into(),
            ..Default::default()
        };
        s.save_settings(&st).unwrap();
        assert_eq!(s.load_settings().unwrap().hotkey, "Ctrl+Space");
    }
}
