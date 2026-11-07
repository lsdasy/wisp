//! Нечёткий поиск по названиям.
//!
//! Шкала (чем больше — тем выше в выдаче):
//! * 100 — название начинается с запроса;
//! * 80  — с запроса начинается одно из слов;
//! * 70  — запрос совпадает с первыми буквами слов («vsc» → Visual Studio Code);
//! * 50  — запрос встречается внутри названия;
//! * 20  — все буквы запроса идут в названии по порядку («tlgm» → Telegram);
//! * 0   — не подходит.
//!
//! Простая шкала вместо алгоритмов вроде fzf выбрана осознанно: названий
//! у нас десятки тысяч, а не миллионы, и предсказуемый порядок для пользователя важнее.

/// `needle` должен быть уже в нижнем регистре.
pub fn score(haystack: &str, needle: &str) -> u32 {
    score_lowered(&haystack.to_lowercase(), needle)
}

/// То же, но строка уже в нижнем регистре — для индекса файлов, где важна каждая аллокация.
pub fn score_lowered(hay: &str, needle: &str) -> u32 {
    if needle.is_empty() {
        return 1;
    }
    if hay.starts_with(needle) {
        return 100;
    }
    let words: Vec<&str> = hay.split(is_separator).filter(|w| !w.is_empty()).collect();
    if words.iter().any(|w| w.starts_with(needle)) {
        return 80;
    }
    let initials: String = words.iter().filter_map(|w| w.chars().next()).collect();
    if initials.starts_with(needle) {
        return 70;
    }
    if hay.contains(needle) {
        return 50;
    }
    if is_subsequence(hay, needle) {
        return 20;
    }
    0
}

/// Лучший результат по нескольким вариантам названия (например, название + ключевые слова).
pub fn best_score<'a>(candidates: impl IntoIterator<Item = &'a str>, needle: &str) -> u32 {
    candidates
        .into_iter()
        .map(|c| score(c, needle))
        .max()
        .unwrap_or(0)
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | '-' | '_' | '(' | ')' | '[' | ']')
}

fn is_subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|n| it.any(|h| h == n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking() {
        assert_eq!(score("Telegram", "tel"), 100);
        assert_eq!(score("Google Chrome", "chr"), 80);
        assert_eq!(score("Visual Studio Code", "vsc"), 70);
        assert_eq!(score("Notepad++", "pad"), 50);
        assert_eq!(score("Telegram", "tlgm"), 20);
        assert_eq!(score("Telegram", "xyz"), 0);
    }

    #[test]
    fn cyrillic() {
        assert_eq!(score("Выключить компьютер", "выкл"), 100);
        assert_eq!(score("Выключить компьютер", "комп"), 80);
    }
}
