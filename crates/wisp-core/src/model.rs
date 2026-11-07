//! Общие типы. `SearchItem` живёт только в Rust (в нём лежит действие),
//! в UI уходит облегчённый `ItemView` — так фронтенд не может подменить,
//! что именно будет выполнено: он передаёт только `id`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    App,
    Command,
    File,
    Folder,
    Calc,
    Shell,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemAction {
    Lock,
    Sleep,
    Shutdown,
    Restart,
    Logout,
    EmptyTrash,
}

impl SystemAction {
    /// Операции, которые нельзя отменить, — требуют подтверждения.
    pub fn is_dangerous(self) -> bool {
        matches!(
            self,
            Self::Shutdown | Self::Restart | Self::Logout | Self::EmptyTrash
        )
    }
}

/// Действия, которые выполняет не ядро, а слой приложения (окна, выход).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InternalAction {
    OpenSettings,
    Reindex,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// `target` — путь к .app/.lnk/исполняемому файлу или `shell:AppsFolder\…` на Windows.
    LaunchApp {
        target: String,
    },
    OpenPath {
        path: PathBuf,
    },
    System {
        action: SystemAction,
    },
    Shell {
        command: String,
    },
    UserCommand {
        command: String,
        shell: bool,
    },
    CopyText {
        text: String,
    },
    OpenUrl {
        url: String,
    },
    Plugin {
        plugin: String,
        keyword: String,
    },
    Internal {
        action: InternalAction,
    },
}

/// Полный результат поиска (хранится в Rust).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub subtitle: String,
    pub danger: bool,
    pub score: u32,
    pub action: Action,
}

/// Цветная плитка с буквами вместо иконки.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub text: String,
    pub bg: String,
    pub fg: String,
}

/// То, что видит UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub subtitle: String,
    pub tag: String,
    pub action_label: String,
    pub danger: bool,
    pub favorite: bool,
    pub tile: Option<Tile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionView {
    pub title: String,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcView {
    pub id: String,
    pub expression: String,
    pub display: String,
    pub words: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub request_id: u64,
    pub cancelled: bool,
    pub calc: Option<CalcView>,
    pub sections: Vec<SectionView>,
}

impl SearchResponse {
    pub fn cancelled(request_id: u64) -> Self {
        Self {
            request_id,
            cancelled: true,
            calc: None,
            sections: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExecOutcome {
    /// Готово; `hide` — спрятать окно.
    Done { message: String, hide: bool },
    /// Скопировать текст в буфер обмена (делает слой приложения).
    Copy { text: String, message: String },
    /// Опасная операция — UI должен показать подтверждение и повторить вызов с `confirmed = true`.
    NeedsConfirmation { id: String, title: String },
    /// Вывод команды.
    #[serde(rename_all = "camelCase")]
    Output {
        title: String,
        stdout: String,
        stderr: String,
        code: Option<i32>,
        timed_out: bool,
    },
    /// Плагин вернул список результатов.
    Items { title: String, items: Vec<ItemView> },
    /// Действие для слоя приложения.
    Internal { action: InternalAction },
}

impl SearchItem {
    pub fn tag(&self) -> &'static str {
        match self.kind {
            ItemKind::App => "Приложение",
            ItemKind::Command if self.danger => "Требует подтверждения",
            ItemKind::Command => "Команда",
            ItemKind::File => "Файл",
            ItemKind::Folder => "Папка",
            ItemKind::Calc => "Калькулятор",
            ItemKind::Shell if self.danger => "Опасная команда",
            ItemKind::Shell => "Shell",
            ItemKind::Plugin => "Расширение",
        }
    }

    pub fn action_label(&self) -> &'static str {
        match (&self.kind, &self.action) {
            (_, Action::CopyText { .. }) => "Скопировать",
            (ItemKind::App | ItemKind::File | ItemKind::Folder, _)
            | (_, Action::OpenUrl { .. }) => "Открыть",
            _ => "Выполнить",
        }
    }

    pub fn view(&self, favorite: bool) -> ItemView {
        ItemView {
            id: self.id.clone(),
            kind: self.kind,
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            tag: self.tag().to_string(),
            action_label: self.action_label().to_string(),
            danger: self.danger,
            favorite,
            tile: (self.kind == ItemKind::App).then(|| tile_for(&self.title)),
        }
    }
}

/// Буквы и цвет плитки выводятся из названия, чтобы у приложения всегда был один и тот же цвет.
pub fn tile_for(title: &str) -> Tile {
    const PALETTE: [(&str, &str); 10] = [
        ("#2F7FD1", "#FFFFFF"),
        ("#1F7FB0", "#FFFFFF"),
        ("#C23B2F", "#FFFFFF"),
        ("#4752C4", "#FFFFFF"),
        ("#8A3FE0", "#FFFFFF"),
        ("#1B7A4B", "#FFFFFF"),
        ("#B4570F", "#FFFFFF"),
        ("#33333D", "#FFFFFF"),
        ("#E0A526", "#2B1E00"),
        ("#5FB3A1", "#06261F"),
    ];
    let words: Vec<&str> = title
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect();
    let text: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(1).flat_map(char::to_uppercase).collect(),
        [a, b, ..] => a
            .chars()
            .take(1)
            .chain(b.chars().take(1))
            .flat_map(char::to_uppercase)
            .collect(),
    };
    let hash = title
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    let (bg, fg) = PALETTE[(hash % PALETTE.len() as u32) as usize];
    Tile {
        text,
        bg: bg.into(),
        fg: fg.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_letters() {
        assert_eq!(tile_for("Visual Studio Code").text, "VS");
        assert_eq!(tile_for("telegram").text, "T");
        assert_eq!(tile_for("Telegram").bg, tile_for("Telegram").bg);
    }
}
