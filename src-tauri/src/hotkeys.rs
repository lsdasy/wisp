//! Глобальная горячая клавиша.

use crate::window;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                window::toggle(app);
            }
        })
        .build()
}

pub fn parse(hotkey: &str) -> Result<Shortcut, String> {
    hotkey
        .parse::<Shortcut>()
        .map_err(|e| format!("не удалось разобрать сочетание «{hotkey}»: {e}"))
}

/// Снимает старую клавишу и регистрирует новую.
pub fn apply(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcut = parse(hotkey)?;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.register(shortcut)
        .map_err(|e| format!("сочетание «{hotkey}» уже занято другой программой ({e})"))
}
