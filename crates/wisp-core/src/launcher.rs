//! Индекс установленных приложений.

use crate::matcher;
use crate::platform::{self, AppEntry};
use crate::{Action, ItemKind, SearchItem};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
pub struct AppIndex {
    apps: RwLock<Vec<AppEntry>>,
}

impl AppIndex {
    /// Пересканировать приложения (вызывается в фоне при старте и по команде «Переиндексировать»).
    pub fn refresh(&self) {
        let found = platform::discover_apps();
        log::info!("найдено приложений: {}", found.len());
        self.set(found);
    }

    pub fn set(&self, apps: Vec<AppEntry>) {
        *self.apps.write().unwrap_or_else(|e| e.into_inner()) = apps;
    }

    pub fn len(&self) -> usize {
        self.apps.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `counts` — сколько раз запускали каждый id: частые приложения поднимаются выше.
    pub fn search(
        &self,
        needle: &str,
        limit: usize,
        counts: &HashMap<String, u32>,
    ) -> Vec<SearchItem> {
        let apps = self.apps.read().unwrap_or_else(|e| e.into_inner());
        let mut found: Vec<SearchItem> = apps
            .iter()
            .filter_map(|a| {
                let base = matcher::score(&a.title, needle);
                (base > 0).then(|| {
                    let mut item = app_item(a);
                    let boost = counts.get(&item.id).map_or(0, |c| (c * 4).min(40));
                    item.score = base + boost;
                    item
                })
            })
            .collect();
        found.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.title.len().cmp(&b.title.len()))
        });
        found.truncate(limit);
        found
    }
}

pub fn app_item(a: &AppEntry) -> SearchItem {
    SearchItem {
        id: format!("app:{}", a.target),
        kind: ItemKind::App,
        title: a.title.clone(),
        subtitle: a.subtitle.clone(),
        danger: false,
        score: 0,
        action: Action::LaunchApp {
            target: a.target.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(t: &str) -> AppEntry {
        AppEntry {
            title: t.into(),
            subtitle: String::new(),
            target: t.into(),
        }
    }

    #[test]
    fn frecency_boost() {
        let idx = AppIndex::default();
        idx.set(vec![entry("Telegram"), entry("Terminal")]);
        assert_eq!(idx.search("te", 5, &HashMap::new())[0].title, "Telegram"); // короче — выше
        let counts = HashMap::from([("app:Terminal".to_string(), 3)]);
        assert_eq!(idx.search("te", 5, &counts)[0].title, "Terminal");
    }
}
