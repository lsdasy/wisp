//! Linux — не основная цель, но поддерживается, чтобы проект собирался и тестировался в CI.

use super::{AppEntry, spawn_detached, split_args};
use crate::{Error, Result, SystemAction};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

struct DesktopEntry {
    name: String,
    exec: String,
    comment: String,
}

fn parse_desktop(text: &str) -> Option<DesktopEntry> {
    let mut in_main = false;
    let (mut name, mut exec, mut comment) = (None, None, String::new());
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "Name" => name = Some(v.trim().to_string()),
            "Exec" => exec = Some(v.trim().to_string()),
            "Comment" if comment.is_empty() => comment = v.trim().to_string(),
            "NoDisplay" | "Hidden" if v.trim() == "true" => return None,
            "Type" if v.trim() != "Application" => return None,
            _ => {}
        }
    }
    Some(DesktopEntry {
        name: name?,
        exec: exec?,
        comment,
    })
}

fn app_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        "/usr/share/applications",
        "/usr/local/share/applications",
        "/var/lib/flatpak/exports/share/applications",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(d) = dirs::data_dir() {
        v.push(d.join("applications"));
    }
    v
}

pub fn discover_apps() -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut apps = vec![];
    for dir in app_dirs() {
        for e in walkdir::WalkDir::new(dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if e.path().extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            let Some(d) = parse_desktop(&text) else {
                continue;
            };
            if seen.insert(d.name.to_lowercase()) {
                apps.push(AppEntry {
                    title: d.name,
                    subtitle: d.comment,
                    target: e.path().to_string_lossy().into_owned(),
                });
            }
        }
    }
    apps
}

pub fn launch_app(target: &str, args: Option<&str>) -> Result<()> {
    let text = std::fs::read_to_string(target)?;
    let d = parse_desktop(&text).ok_or_else(|| Error::Open(target.into()))?;
    // Убираем коды полей %f %U и т.п. — аргументы подставляем сами.
    let mut parts: Vec<String> = shlex::split(&d.exec)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !(p.len() == 2 && p.starts_with('%')))
        .collect();
    if parts.is_empty() {
        return Err(Error::Open(target.into()));
    }
    parts.extend(split_args(args));
    let mut c = Command::new(&parts[0]);
    c.args(&parts[1..]);
    spawn_detached(c)
}

pub fn reveal(path: &Path) -> Result<()> {
    let dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    open::that_detached(dir).map_err(|e| Error::Open(e.to_string()))
}

pub fn system_action(action: SystemAction) -> Result<()> {
    let (prog, args): (&str, &[&str]) = match action {
        SystemAction::Lock => ("loginctl", &["lock-session"]),
        SystemAction::Sleep => ("systemctl", &["suspend"]),
        SystemAction::Shutdown => ("systemctl", &["poweroff"]),
        SystemAction::Restart => ("systemctl", &["reboot"]),
        SystemAction::Logout => ("loginctl", &["terminate-session", "self"]),
        SystemAction::EmptyTrash => ("gio", &["trash", "--empty"]),
    };
    let mut c = Command::new(prog);
    c.args(args);
    spawn_detached(c)
}

pub fn shell_command(command: &str) -> tokio::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut c = tokio::process::Command::new(shell);
    c.arg("-c").arg(command);
    c
}

pub fn program_command(program: &str) -> tokio::process::Command {
    tokio::process::Command::new(program)
}

pub const SHELL_NAME: &str = "Shell";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_file() {
        let d = parse_desktop("[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\n")
            .unwrap();
        assert_eq!(d.name, "Firefox");
        assert!(parse_desktop("[Desktop Entry]\nName=X\nExec=x\nNoDisplay=true\n").is_none());
    }
}
