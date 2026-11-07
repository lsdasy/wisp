//! Windows.

use super::{AppEntry, spawn_detached};
use crate::{Error, Result, SystemAction};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Не показывать чёрное окно консоли у дочерних процессов.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hidden(program: &str) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// ShellExecuteW понимает и .exe, и .lnk, и `shell:AppsFolder\…` (приложения из Store).
fn shell_execute(file: &str, params: Option<&str>) -> Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let op = wide(OsStr::new("open"));
    let file_w = wide(OsStr::new(file));
    let params_w = params
        .filter(|p| !p.trim().is_empty())
        .map(|p| wide(OsStr::new(p)));
    // SAFETY: все строки — валидные нуль-терминированные UTF-16 буферы, живущие до конца вызова.
    let h = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file_w.as_ptr(),
            params_w.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // По документации значение > 32 означает успех.
    if (h as isize) > 32 {
        Ok(())
    } else {
        Err(Error::Open(format!("{file} (код {})", h as isize)))
    }
}

pub fn discover_apps() -> Vec<AppEntry> {
    let apps = start_apps().unwrap_or_default();
    if apps.is_empty() {
        scan_start_menu()
    } else {
        apps
    }
}

/// `Get-StartApps` возвращает и обычные программы, и приложения из Microsoft Store.
fn start_apps() -> Option<Vec<AppEntry>> {
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "AppID")]
        app_id: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Rows {
        Many(Vec<Row>),
        One(Row),
    }

    let out = hidden("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress",
        ])
        .output()
        .ok()?;
    let bytes = out
        .stdout
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&out.stdout);
    let rows = match serde_json::from_slice::<Rows>(bytes).ok()? {
        Rows::Many(v) => v,
        Rows::One(r) => vec![r],
    };

    let mut seen = HashSet::new();
    Some(
        rows.into_iter()
            .filter(|r| !is_noise(&r.name) && seen.insert(r.name.to_lowercase()))
            .map(|r| {
                let subtitle =
                    if r.app_id.contains('\\') || r.app_id.to_lowercase().ends_with(".exe") {
                        Path::new(&r.app_id)
                            .file_name()
                            .map_or_else(String::new, |f| f.to_string_lossy().into_owned())
                    } else {
                        "Microsoft Store".into()
                    };
                AppEntry {
                    title: r.name,
                    subtitle,
                    target: format!("shell:AppsFolder\\{}", r.app_id),
                }
            })
            .collect(),
    )
}

fn scan_start_menu() -> Vec<AppEntry> {
    let roots: Vec<PathBuf> = ["ProgramData", "APPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"))
        .collect();
    let mut seen = HashSet::new();
    let mut apps = vec![];
    for root in roots {
        for e in walkdir::WalkDir::new(root)
            .max_depth(4)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = e.path();
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(ext.as_str(), "lnk" | "url" | "appref-ms") {
                continue;
            }
            let Some(title) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if is_noise(&title) || !seen.insert(title.to_lowercase()) {
                continue;
            }
            apps.push(AppEntry {
                title,
                subtitle: "Ярлык".into(),
                target: p.to_string_lossy().into_owned(),
            });
        }
    }
    apps
}

/// Деинсталляторы и справка только засоряют выдачу.
fn is_noise(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "uninstall",
        "удалить",
        "удаление",
        "readme",
        "release notes",
        "documentation",
    ]
    .iter()
    .any(|w| n.contains(w))
}

pub fn launch_app(target: &str, args: Option<&str>) -> Result<()> {
    shell_execute(target, args)
}

/// Показать файл в Проводнике с выделением.
pub fn reveal(path: &Path) -> Result<()> {
    let mut c = Command::new("explorer.exe");
    c.raw_arg(format!("/select,\"{}\"", path.display()));
    spawn_detached(c)
}

pub fn system_action(action: SystemAction) -> Result<()> {
    let c = match action {
        SystemAction::Lock => {
            let mut c = hidden("rundll32.exe");
            c.arg("user32.dll,LockWorkStation");
            c
        }
        SystemAction::Sleep => {
            let mut c = hidden("rundll32.exe");
            c.arg("powrprof.dll,SetSuspendState").arg("0,1,0");
            c
        }
        SystemAction::Shutdown => {
            let mut c = hidden("shutdown.exe");
            c.args(["/s", "/t", "0"]);
            c
        }
        SystemAction::Restart => {
            let mut c = hidden("shutdown.exe");
            c.args(["/r", "/t", "0"]);
            c
        }
        SystemAction::Logout => {
            let mut c = hidden("shutdown.exe");
            c.arg("/l");
            c
        }
        SystemAction::EmptyTrash => {
            let mut c = hidden("powershell.exe");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Clear-RecycleBin -Force -ErrorAction SilentlyContinue",
            ]);
            c
        }
    };
    spawn_detached(c)
}

/// Команда для `> …` и алиасов: PowerShell с выводом в UTF-8.
pub fn shell_command(command: &str) -> tokio::process::Command {
    let script = format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; {command}");
    let mut c = tokio::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(script);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Для плагинов: прямой запуск программы без shell.
pub fn program_command(program: &str) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

pub const SHELL_NAME: &str = "PowerShell";
