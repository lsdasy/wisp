//! Окна: показ/скрытие лаунчера, позиционирование, эффекты прозрачности, окно настроек.

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const MAIN: &str = "main";
pub const SETTINGS: &str = "settings";

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN)
}

/// Горячая клавиша: показать, а если уже открыто — попросить UI спрятаться (с анимацией).
pub fn toggle(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
        let _ = w.emit("lumen://hide-requested", ());
    } else {
        show(app);
    }
}

pub fn show(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    position_on_cursor_monitor(&w);
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
    let _ = w.emit("lumen://shown", ());
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = main_window(app) {
        let _ = w.hide();
    }
}

/// Окно появляется на том мониторе, где сейчас курсор, на ~18% высоты сверху — как у Raycast.
fn position_on_cursor_monitor(w: &WebviewWindow) {
    let (Ok(cursor), Ok(monitors), Ok(size)) =
        (w.cursor_position(), w.available_monitors(), w.outer_size())
    else {
        return;
    };
    let contains = |m: &&tauri::Monitor| {
        let (p, s) = (m.position(), m.size());
        cursor.x >= f64::from(p.x)
            && cursor.x < f64::from(p.x) + f64::from(s.width)
            && cursor.y >= f64::from(p.y)
            && cursor.y < f64::from(p.y) + f64::from(s.height)
    };
    let Some(m) = monitors.iter().find(contains).or(monitors.first()) else {
        return;
    };
    let (p, s) = (m.position(), m.size());
    let x = p.x + (s.width as i32 - size.width as i32) / 2;
    let y = p.y + (f64::from(s.height) * 0.18) as i32;
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// Размытие фона окна системными средствами. Возвращает `true`, если эффект применён —
/// тогда UI делает фон более прозрачным.
pub fn apply_effects(w: &WebviewWindow, dark: bool) -> bool {
    #[cfg(target_os = "windows")]
    {
        let _ = window_vibrancy::clear_acrylic(w);
        let tint = if dark {
            (24, 24, 28, 150)
        } else {
            (244, 244, 248, 150)
        };
        return window_vibrancy::apply_acrylic(w, Some(tint)).is_ok();
    }
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState};
        let _ = window_vibrancy::clear_vibrancy(w);
        let material = if dark {
            NSVisualEffectMaterial::HudWindow
        } else {
            NSVisualEffectMaterial::Popover
        };
        return window_vibrancy::apply_vibrancy(
            w,
            material,
            Some(NSVisualEffectState::Active),
            Some(14.0),
        )
        .is_ok();
    }
    #[allow(unreachable_code)]
    {
        let _ = (w, dark);
        false
    }
}

pub fn open_settings(app: &AppHandle) {
    hide(app);
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let built =
        WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("index.html#settings".into()))
            .title("Настройки Lumen")
            .inner_size(900.0, 620.0)
            .min_inner_size(720.0, 480.0)
            .center()
            .focused(true)
            .build();
    if let Err(e) = built {
        log::error!("не удалось открыть настройки: {e}");
    }
}
