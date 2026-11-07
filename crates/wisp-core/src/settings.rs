use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

/// Пользовательская команда-алиас: «gs» → `git status` в нужной папке и т.п.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCommand {
    pub keyword: String,
    pub title: String,
    /// Что выполнить. При `shell = false` — путь/ссылка для открытия.
    pub command: String,
    #[serde(default)]
    pub shell: bool,
    /// Всегда спрашивать подтверждение перед запуском.
    #[serde(default)]
    pub confirm: bool,
}

/// Разрешения, которые пользователь выдал конкретному плагину.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginGrant {
    pub enabled: bool,
    pub allow_exec: bool,
    pub allow_open: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Providers {
    pub apps: bool,
    pub files: bool,
    pub commands: bool,
    pub calculator: bool,
    pub plugins: bool,
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            apps: true,
            files: true,
            commands: true,
            calculator: true,
            plugins: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Формат tauri-plugin-global-shortcut: "Alt+Space", "CommandOrControl+Shift+K".
    pub hotkey: String,
    pub theme: Theme,
    pub autostart: bool,
    pub animations: bool,
    pub hide_on_blur: bool,
    pub record_history: bool,
    pub confirm_dangerous: bool,
    pub show_command_output: bool,
    pub max_results_per_section: usize,
    pub index_roots: Vec<PathBuf>,
    pub index_excludes: Vec<String>,
    pub providers: Providers,
    pub aliases: Vec<UserCommand>,
    pub plugins: BTreeMap<String, PluginGrant>,
}

impl Default for Settings {
    fn default() -> Self {
        let roots = [
            dirs::document_dir(),
            dirs::desktop_dir(),
            dirs::download_dir(),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            hotkey: "Alt+Space".into(),
            theme: Theme::System,
            autostart: false,
            animations: true,
            hide_on_blur: true,
            record_history: true,
            confirm_dangerous: true,
            show_command_output: true,
            max_results_per_section: 5,
            index_roots: roots,
            index_excludes: [
                "node_modules",
                "target",
                ".git",
                ".svn",
                ".hg",
                "__pycache__",
                ".venv",
                "venv",
                "AppData",
                "$Recycle.Bin",
                ".Trash",
                "Library",
                ".cache",
                "*.tmp",
            ]
            .map(String::from)
            .to_vec(),
            providers: Providers::default(),
            aliases: vec![],
            plugins: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Приводит настройки к безопасным значениям — их может прислать UI или руками поправленная БД.
    pub fn sanitized(mut self) -> Self {
        self.max_results_per_section = self.max_results_per_section.clamp(1, 20);
        self.index_roots.retain(|p| p.is_absolute());
        self.index_roots.dedup();
        self.index_excludes.retain(|e| !e.trim().is_empty());
        self.aliases
            .retain(|a| !a.keyword.trim().is_empty() && !a.command.trim().is_empty());
        for a in &mut self.aliases {
            a.keyword = a.keyword.trim().to_lowercase();
            if a.title.trim().is_empty() {
                a.title = a.keyword.clone();
            }
        }
        if self.hotkey.trim().is_empty() {
            self.hotkey = Self::default().hotkey;
        }
        self
    }
}
