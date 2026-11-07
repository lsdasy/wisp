//! Системные команды, пользовательские алиасы и выполнение PowerShell / shell.

use crate::platform;
use crate::settings::{Settings, UserCommand};
use crate::{Action, InternalAction, ItemKind, Result, SearchItem, SystemAction, matcher};
use std::process::Stdio;
use std::time::Duration;

struct Builtin {
    id: &'static str,
    title: &'static str,
    keywords: &'static [&'static str],
    action: Action,
}

fn builtins() -> Vec<Builtin> {
    use InternalAction as I;
    use SystemAction as S;
    let sys = |action| Action::System { action };
    let int = |action| Action::Internal { action };
    vec![
        Builtin {
            id: "lock",
            title: "Заблокировать компьютер",
            keywords: &["lock", "блок"],
            action: sys(S::Lock),
        },
        Builtin {
            id: "sleep",
            title: "Спящий режим",
            keywords: &["sleep", "сон"],
            action: sys(S::Sleep),
        },
        Builtin {
            id: "shutdown",
            title: "Выключить компьютер",
            keywords: &["shutdown", "power off", "выкл"],
            action: sys(S::Shutdown),
        },
        Builtin {
            id: "restart",
            title: "Перезагрузить",
            keywords: &["restart", "reboot", "перезагрузка"],
            action: sys(S::Restart),
        },
        Builtin {
            id: "logout",
            title: "Выйти из системы",
            keywords: &["logout", "log out", "sign out"],
            action: sys(S::Logout),
        },
        Builtin {
            id: "trash",
            title: "Очистить корзину",
            keywords: &["empty trash", "recycle bin", "корзина"],
            action: sys(S::EmptyTrash),
        },
        Builtin {
            id: "settings",
            title: "Настройки Wisp",
            keywords: &["settings", "preferences", "параметры"],
            action: int(I::OpenSettings),
        },
        Builtin {
            id: "reindex",
            title: "Переиндексировать файлы и приложения",
            keywords: &["reindex", "индекс"],
            action: int(I::Reindex),
        },
        Builtin {
            id: "quit",
            title: "Выйти из Wisp",
            keywords: &["quit", "exit", "закрыть"],
            action: int(I::Quit),
        },
    ]
}

fn builtin_item(b: &Builtin, score: u32) -> SearchItem {
    let (danger, subtitle) = match &b.action {
        Action::System { action } => (action.is_dangerous(), "Система"),
        _ => (false, "Wisp"),
    };
    SearchItem {
        id: format!("cmd:{}", b.id),
        kind: ItemKind::Command,
        title: b.title.into(),
        subtitle: subtitle.into(),
        danger,
        score,
        action: b.action.clone(),
    }
}

fn alias_item(a: &UserCommand, args: Option<&str>, score: u32) -> SearchItem {
    let subtitle = match args {
        Some(args) => format!("{} · аргументы: {args}", a.command),
        None => a.command.clone(),
    };
    SearchItem {
        id: format!("alias:{}", a.keyword),
        kind: ItemKind::Command,
        title: a.title.clone(),
        subtitle,
        danger: a.confirm || (a.shell && is_dangerous_shell(&a.command)),
        score,
        action: Action::UserCommand {
            command: a.command.clone(),
            shell: a.shell,
        },
    }
}

/// `needle` — запрос в нижнем регистре. Первое слово, совпавшее с ключевым словом алиаса,
/// даёт точное попадание, остальное — аргументы.
pub fn search(needle: &str, settings: &Settings, limit: usize) -> Vec<SearchItem> {
    let mut out: Vec<SearchItem> = builtins()
        .iter()
        .filter_map(|b| {
            let s = matcher::best_score(
                std::iter::once(b.title).chain(b.keywords.iter().copied()),
                needle,
            );
            (s > 0).then(|| builtin_item(b, s))
        })
        .collect();

    let (first, rest) = needle.split_once(' ').map_or((needle, None), |(f, r)| {
        (f, Some(r.trim()).filter(|r| !r.is_empty()))
    });
    for a in &settings.aliases {
        if a.keyword == first {
            out.push(alias_item(a, rest, 110));
        } else {
            let s = matcher::best_score([a.title.as_str(), a.keyword.as_str()], needle);
            if s > 0 {
                out.push(alias_item(a, None, s));
            }
        }
    }
    out.sort_by_key(|i| std::cmp::Reverse(i.score));
    out.truncate(limit);
    out
}

/// Что показывать при пустом запросе.
pub fn suggestions() -> Vec<SearchItem> {
    builtins()
        .iter()
        .filter(|b| matches!(b.id, "settings" | "lock"))
        .map(|b| builtin_item(b, 0))
        .collect()
}

pub fn shell_item(command: &str) -> SearchItem {
    SearchItem {
        id: format!("shell:{command}"),
        kind: ItemKind::Shell,
        title: command.into(),
        subtitle: platform::SHELL_NAME.into(),
        danger: is_dangerous_shell(command),
        score: 100,
        action: Action::Shell {
            command: command.into(),
        },
    }
}

/// Эвристика, а не защита: она лишь решает, просить ли второе подтверждение.
/// Любая shell-команда и так выполняется только по явному Enter.
pub fn is_dangerous_shell(command: &str) -> bool {
    const WORDS: &[&str] = &[
        "rm",
        "rmdir",
        "rd",
        "del",
        "erase",
        "format",
        "mkfs",
        "dd",
        "diskpart",
        "shutdown",
        "reboot",
        "halt",
        "poweroff",
        "remove-item",
        "clear-recyclebin",
        "stop-computer",
        "restart-computer",
        "format-volume",
        "clear-disk",
        "remove-partition",
        "kill",
        "killall",
        "taskkill",
        "stop-process",
        "sudo",
        "chmod",
        "chown",
        "reg",
        "bcdedit",
        "cipher",
        "set-executionpolicy",
        "invoke-expression",
        "iex",
        "launchctl",
    ];
    let lower = command.to_lowercase();
    let tokens = lower.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'));
    let has_word = tokens.into_iter().any(|t| WORDS.contains(&t));
    // `curl … | sh`, перезапись файлов через `>`
    let pipes_to_shell = [
        "| sh",
        "|sh",
        "| bash",
        "|bash",
        "| zsh",
        "| pwsh",
        "| powershell",
    ]
    .iter()
    .any(|p| lower.contains(p));
    let redirects = lower.contains('>');
    has_word || pipes_to_shell || redirects
}

/// Подставляет аргументы в команду алиаса: `{args}` → аргументы, иначе дописывает в конец.
pub fn expand_alias(command: &str, args: Option<&str>) -> String {
    match args {
        Some(a) if command.contains("{args}") => command.replace("{args}", a),
        Some(a) => format!("{command} {a}"),
        None => command.replace("{args}", ""),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub timed_out: bool,
}

const OUTPUT_LIMIT: usize = 64 * 1024;

fn truncate_lossy(bytes: &[u8]) -> String {
    let mut s = String::from_utf8_lossy(bytes).into_owned();
    if s.len() > OUTPUT_LIMIT {
        let mut cut = OUTPUT_LIMIT;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push_str("\n… вывод обрезан");
    }
    s
}

/// Запускает процесс, собирает вывод и убивает его по таймауту.
pub async fn run_captured(mut cmd: tokio::process::Command, timeout: Duration) -> Result<Captured> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = cmd.spawn()?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(out) => {
            let out = out?;
            Ok(Captured {
                stdout: truncate_lossy(&out.stdout),
                stderr: truncate_lossy(&out.stderr),
                code: out.status.code(),
                timed_out: false,
            })
        }
        // future с дочерним процессом уничтожен → kill_on_drop завершает процесс
        Err(_) => Ok(Captured {
            timed_out: true,
            ..Default::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn danger_heuristic() {
        assert!(is_dangerous_shell("Remove-Item C:\\x -Recurse"));
        assert!(is_dangerous_shell("rm -rf ~/tmp"));
        assert!(is_dangerous_shell("curl https://x | sh"));
        assert!(is_dangerous_shell("echo hi > file.txt"));
        assert!(!is_dangerous_shell("Get-Process"));
        assert!(!is_dangerous_shell("git status"));
        assert!(!is_dangerous_shell("ls -la")); // «rm» внутри слова не считается
    }

    #[test]
    fn builtins_found() {
        let s = Settings::default();
        let r = search("выкл", &s, 5);
        assert_eq!(r[0].id, "cmd:shutdown");
        assert!(r[0].danger);
        assert!(!search("lock", &s, 5)[0].danger);
    }

    #[test]
    fn aliases() {
        let mut s = Settings::default();
        s.aliases.push(UserCommand {
            keyword: "gs".into(),
            title: "Git status".into(),
            command: "git status".into(),
            shell: true,
            confirm: false,
        });
        let r = search("gs --short", &s, 5);
        assert_eq!(r[0].id, "alias:gs");
        assert_eq!(
            expand_alias("git status", Some("--short")),
            "git status --short"
        );
        assert_eq!(
            expand_alias("code {args} -n", Some("C:\\p")),
            "code C:\\p -n"
        );
    }

    #[tokio::test]
    async fn captures_output() {
        let out = run_captured(
            platform::shell_command("echo привет"),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert!(out.stdout.contains("привет"));
        assert_eq!(out.code, Some(0));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills() {
        let out = run_captured(
            platform::shell_command("sleep 5"),
            Duration::from_millis(200),
        )
        .await
        .unwrap();
        assert!(out.timed_out);
    }
}
