//! Команды, которые вызывает интерфейс через `invoke`.
//! Тонкий слой: вся логика — в `wisp-core`, здесь только то, что связано с окнами и ОС-интеграцией Tauri.

use crate::{AppState, hotkeys, window};
use wisp_core::plugins::PluginInfo;
use wisp_core::{Error, ExecOutcome, InternalAction, SearchResponse, Settings, engine::Status};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;

type CmdResult<T> = Result<T, Error>;

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    query: String,
    request_id: u64,
) -> CmdResult<SearchResponse> {
    let engine = state.engine.clone();
    // Поиск синхронный (обход индекса), поэтому уводим его с потоков async-рантайма.
    tauri::async_runtime::spawn_blocking(move || engine.search(&query, request_id))
        .await
        .map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
pub async fn execute(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    confirmed: bool,
    query: String,
) -> CmdResult<ExecOutcome> {
    let outcome = state.engine.execute(&id, confirmed, &query).await?;
    Ok(match outcome {
        ExecOutcome::Copy { text, message } => {
            app.clipboard()
                .write_text(text)
                .map_err(|e| Error::Other(e.to_string()))?;
            ExecOutcome::Done {
                message,
                hide: true,
            }
        }
        ExecOutcome::Internal { action } => {
            match action {
                InternalAction::OpenSettings => window::open_settings(&app),
                InternalAction::Reindex => state.engine.reindex(),
                InternalAction::Quit => app.exit(0),
            }
            ExecOutcome::Done {
                message: "Готово".into(),
                hide: action != InternalAction::Reindex,
            }
        }
        other => other,
    })
}

#[tauri::command]
pub fn reveal(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.engine.reveal(&id)
}

#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    state.engine.toggle_favorite(&id)
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.engine.settings()
}

/// Сначала применяем то, что может не получиться (горячая клавиша), и только потом сохраняем.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    let old = state.engine.settings();
    if old.hotkey != settings.hotkey {
        if let Err(e) = hotkeys::apply(&app, &settings.hotkey) {
            let _ = hotkeys::apply(&app, &old.hotkey);
            return Err(Error::Other(e));
        }
    }
    if old.autostart != settings.autostart {
        let al = app.autolaunch();
        let r = if settings.autostart {
            al.enable()
        } else {
            al.disable()
        };
        if let Err(e) = r {
            log::warn!("автозапуск: {e}");
        }
    }
    let saved = state.engine.update_settings(settings)?;
    let _ = app.emit("wisp://settings-changed", &saved);
    Ok(saved)
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Status {
    state.engine.status()
}

#[tauri::command]
pub fn reindex(state: State<'_, AppState>) {
    state.engine.reindex();
}

#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> Vec<PluginInfo> {
    state.engine.plugins()
}

#[tauri::command]
pub fn reload_plugins(state: State<'_, AppState>) -> Vec<PluginInfo> {
    state.engine.reload_plugins();
    state.engine.plugins()
}

#[tauri::command]
pub fn open_plugins_dir(state: State<'_, AppState>) -> CmdResult<()> {
    let dir = std::path::PathBuf::from(state.engine.status().plugins_dir);
    std::fs::create_dir_all(&dir)?;
    wisp_core::platform::open_path(&dir)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.clear_history()
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    window::hide(&app);
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    window::open_settings(&app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    platform: &'static str,
    version: String,
    vibrancy: bool,
}

#[tauri::command]
pub fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        platform: std::env::consts::OS,
        version: app.package_info().version.to_string(),
        vibrancy: state.vibrancy.load(Ordering::Relaxed),
    }
}

/// UI сообщает итоговую тему (с учётом «Системной»), чтобы перекрасить системное размытие.
#[tauri::command]
pub fn set_effective_theme(app: AppHandle, state: State<'_, AppState>, dark: bool) -> bool {
    let Some(w) = app.get_webview_window(window::MAIN) else {
        return false;
    };
    let ok = window::apply_effects(&w, dark);
    state.vibrancy.store(ok, Ordering::Relaxed);
    ok
}
