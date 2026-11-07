//! macOS.

use super::{AppEntry, spawn_detached, split_args};
use crate::{Result, SystemAction};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn discover_apps() -> Vec<AppEntry> {
    let mut roots: Vec<PathBuf> = [
        "/Applications",
        "/System/Applications",
        "/System/Applications/Utilities",
        "/System/Library/CoreServices/Applications",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots.push(PathBuf::from("/System/Library/CoreServices/Finder.app"));

    let mut seen = HashSet::new();
    let mut apps = vec![];
    for root in roots {
        // Глубина 2: /Applications/X.app и /Applications/Папка/Y.app. Внутрь .app дальше Contents не уходим.
        for e in walkdir::WalkDir::new(&root)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "app") || !e.file_type().is_dir() {
                continue;
            }
            let Some(title) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if !seen.insert(title.to_lowercase()) {
                continue;
            }
            let subtitle = p
                .parent()
                .map_or_else(String::new, |d| d.display().to_string());
            apps.push(AppEntry {
                title,
                subtitle,
                target: p.to_string_lossy().into_owned(),
            });
        }
    }
    apps
}

pub fn launch_app(target: &str, args: Option<&str>) -> Result<()> {
    let mut c = Command::new("open");
    c.arg("-a").arg(target);
    let args = split_args(args);
    if !args.is_empty() {
        c.arg("--args").args(args);
    }
    spawn_detached(c)
}

pub fn reveal(path: &Path) -> Result<()> {
    let mut c = Command::new("open");
    c.arg("-R").arg(path);
    spawn_detached(c)
}

fn osascript(script: &str) -> Command {
    let mut c = Command::new("osascript");
    c.arg("-e").arg(script);
    c
}

pub fn system_action(action: SystemAction) -> Result<()> {
    let c = match action {
        SystemAction::Lock => {
            let mut c = Command::new("pmset");
            c.arg("displaysleepnow");
            c
        }
        SystemAction::Sleep => {
            let mut c = Command::new("pmset");
            c.arg("sleepnow");
            c
        }
        SystemAction::Shutdown => osascript(r#"tell application "System Events" to shut down"#),
        SystemAction::Restart => osascript(r#"tell application "System Events" to restart"#),
        SystemAction::Logout => osascript(r#"tell application "System Events" to log out"#),
        SystemAction::EmptyTrash => osascript(r#"tell application "Finder" to empty trash"#),
    };
    spawn_detached(c)
}

pub fn shell_command(command: &str) -> tokio::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut c = tokio::process::Command::new(shell);
    c.arg("-lc").arg(command);
    c
}

pub fn program_command(program: &str) -> tokio::process::Command {
    tokio::process::Command::new(program)
}

pub const SHELL_NAME: &str = "Shell";
