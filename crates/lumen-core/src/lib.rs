//! Ядро Lumen: всё, что не зависит от UI и Tauri.
//!
//! Ядро ничего не знает об окнах и горячих клавишах — это делает слой `src-tauri`.
//! Благодаря этому всю логику поиска можно тестировать обычным `cargo test`.
//!
//! Модули:
//! * [`model`]      — типы, которыми ядро обменивается с UI;
//! * [`engine`]     — центральная логика (`core`): разбор запроса, опрос провайдеров, выполнение действий;
//! * [`matcher`]    — нечёткое сравнение строк (`search`);
//! * [`calculator`] — разбор и вычисление выражений;
//! * [`launcher`]   — поиск установленных приложений;
//! * [`filesystem`] — индекс файлов и отслеживание изменений;
//! * [`commands`]   — системные команды, алиасы, PowerShell / shell;
//! * [`plugins`]    — расширения на основе манифестов;
//! * [`storage`]    — SQLite: история, избранное, настройки;
//! * [`settings`]   — структура настроек;
//! * [`platform`]   — всё, что отличается между Windows, macOS и Linux.

pub mod calculator;
pub mod commands;
pub mod engine;
pub mod error;
pub mod filesystem;
pub mod launcher;
pub mod matcher;
pub mod model;
pub mod platform;
pub mod plugins;
pub mod settings;
pub mod storage;

pub use engine::Engine;
pub use error::{Error, Result};
pub use model::*;
pub use settings::Settings;
