//! Слой приложения Tauri: окна, трей, горячие клавиши, автозапуск.
//! Логика поиска и выполнения — в `lumen-core`.

mod commands;
mod hotkeys;
mod tray;
mod window;

use lumen_core::Engine;
use lumen_core::settings::Theme;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

/// Аргумент, с которым Lumen запускается из автозагрузки: стартует свернутым в трей.
const HIDDEN_ARG: &str = "--hidden";

pub struct AppState {
    pub engine: Arc<Engine>,
    pub vibrancy: AtomicBool,
}

pub fn run() {
    tauri::Builder::default()
        // Второй запуск не создаёт копию, а показывает уже работающее окно. Должен быть первым плагином.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show(app)
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(hotkeys::plugin())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // На macOS лаунчер живёт в строке меню, без значка в Dock.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let engine = Engine::open(&app.path().app_data_dir()?)?;
            engine.start_background();
            let settings = engine.settings();

            if let Err(e) = hotkeys::apply(app.handle(), &settings.hotkey) {
                log::warn!("{e}");
            }
            tray::create(app.handle())?;

            let mut vibrancy = false;
            if let Some(w) = window::main_window(app.handle()) {
                let dark = match settings.theme {
                    Theme::Dark => true,
                    Theme::Light => false,
                    Theme::System => w.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(true),
                };
                vibrancy = window::apply_effects(&w, dark);
            }
            app.manage(AppState {
                engine,
                vibrancy: AtomicBool::new(vibrancy),
            });

            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                window::show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|w, event| {
            if w.label() != window::MAIN {
                return;
            }
            match event {
                // Крестик/Alt+F4 прячут лаунчер, а не завершают приложение.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = w.hide();
                }
                WindowEvent::Focused(false) => {
                    let hide = w
                        .app_handle()
                        .try_state::<AppState>()
                        .is_some_and(|s| s.engine.settings().hide_on_blur);
                    if hide {
                        let _ = w.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::execute,
            commands::reveal,
            commands::toggle_favorite,
            commands::get_settings,
            commands::save_settings,
            commands::get_status,
            commands::reindex,
            commands::list_plugins,
            commands::reload_plugins,
            commands::open_plugins_dir,
            commands::clear_history,
            commands::hide_window,
            commands::open_settings,
            commands::app_info,
            commands::set_effective_theme,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Lumen");
}
