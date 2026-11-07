//! Платформенный слой. У каждой ОС свой файл с одинаковым набором функций,
//! остальной код вызывает только то, что реэкспортировано отсюда.

use crate::{Error, Result};
use std::path::Path;
use std::process::Command;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(windows)]
pub use windows::*;

/// Найденное приложение.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub title: String,
    pub subtitle: String,
    /// Что передать в [`launch_app`]: путь к .app / .lnk / .desktop или `shell:AppsFolder\…`.
    pub target: String,
}

/// Открывает файл или папку программой по умолчанию.
pub fn open_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(Error::Open(format!(
            "{} больше не существует",
            path.display()
        )));
    }
    open::that_detached(path).map_err(|e| Error::Open(e.to_string()))
}

/// Открывает только http(s)-ссылки — `file:`, `javascript:` и прочие схемы отбрасываются.
pub fn open_url(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(Error::Forbidden(format!(
            "разрешены только http(s)-ссылки: {url}"
        )));
    }
    open::that_detached(url).map_err(|e| Error::Open(e.to_string()))
}

/// Запускает процесс и не ждёт его; поток-сторож забирает код завершения, чтобы не копились «зомби».
pub(crate) fn spawn_detached(mut cmd: Command) -> Result<()> {
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Делит строку аргументов как shell (с кавычками), но без запуска shell.
pub fn split_args(args: Option<&str>) -> Vec<String> {
    args.and_then(shlex::split).unwrap_or_default()
}
