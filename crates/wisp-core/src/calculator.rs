//! Калькулятор: рекурсивный спуск, без `eval` и сторонних интерпретаторов.
//!
//! Грамматика:
//! ```text
//! expr    = term   (('+' | '-') term)*
//! term    = unary  (('*' | '/') unary)*
//! unary   = ('-' | '+') unary | power
//! power   = postfix ('^' unary)?
//! postfix = primary '%'?
//! primary = number | '(' expr ')' | func '(' expr ')' | const
//! ```
//! Проценты работают «как в калькуляторе»: `200 + 15%` = 230, `200 * 15%` = 30.

type Func = fn(f64) -> f64;

const FUNCS: &[(&str, Func)] = &[
    ("sqrt", f64::sqrt),
    ("cbrt", f64::cbrt),
    ("sin", f64::sin),
    ("cos", f64::cos),
    ("tan", f64::tan),
    ("asin", f64::asin),
    ("acos", f64::acos),
    ("atan", f64::atan),
    ("ln", f64::ln),
    ("log", f64::log10),
    ("log2", f64::log2),
    ("abs", f64::abs),
    ("round", f64::round),
    ("floor", f64::floor),
    ("ceil", f64::ceil),
    ("exp", f64::exp),
];

#[derive(Debug, Clone, PartialEq)]
pub struct CalcResult {
    pub value: f64,
    /// Для показа: «1 234,5».
    pub display: String,
    /// Для буфера обмена: «1234.5».
    pub copy: String,
}

#[derive(Clone, Copy)]
struct Val {
    v: f64,
    pct: bool,
}

impl Val {
    fn num(v: f64) -> Self {
        Self { v, pct: false }
    }
}

struct Parser {
    s: Vec<char>,
    i: usize,
}

type P<T> = Option<T>;

impl Parser {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> P<Val> {
        let mut a = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let b = self.term()?;
            let a_v = if a.pct { a.v / 100.0 } else { a.v };
            let b_v = if b.pct { a_v * b.v / 100.0 } else { b.v };
            a = Val::num(if op == '+' { a_v + b_v } else { a_v - b_v });
        }
        Some(a)
    }

    fn term(&mut self) -> P<Val> {
        let mut a = self.unary()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.i += 1;
            let b = self.unary()?;
            let av = if a.pct { a.v / 100.0 } else { a.v };
            let bv = if b.pct { b.v / 100.0 } else { b.v };
            a = Val::num(if op == '*' { av * bv } else { av / bv });
        }
        Some(a)
    }

    fn unary(&mut self) -> P<Val> {
        if self.eat('-') {
            let a = self.unary()?;
            return Some(Val {
                v: -a.v,
                pct: a.pct,
            });
        }
        if self.eat('+') {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> P<Val> {
        let a = self.postfix()?;
        if self.eat('^') {
            let b = self.unary()?;
            return Some(Val::num(a.v.powf(b.v)));
        }
        Some(a)
    }

    fn postfix(&mut self) -> P<Val> {
        let a = self.primary()?;
        if self.eat('%') {
            return Some(Val { v: a.v, pct: true });
        }
        Some(a)
    }

    fn primary(&mut self) -> P<Val> {
        let c = self.peek()?;
        if c.is_ascii_digit() || c == '.' {
            let start = self.i;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
                self.i += 1;
            }
            // экспонента: 1e3, 2.5e-4
            if self.peek() == Some('e')
                && matches!(self.s.get(self.i + 1), Some(c) if c.is_ascii_digit() || *c == '-' || *c == '+')
            {
                self.i += 2;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.i += 1;
                }
            }
            let text: String = self.s[start..self.i].iter().collect();
            return text.parse().ok().map(Val::num);
        }
        if self.eat('(') {
            let a = self.expr()?;
            return self.eat(')').then_some(Val::num(a.v));
        }
        if c.is_ascii_alphabetic() {
            let start = self.i;
            while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
                self.i += 1;
            }
            let name: String = self.s[start..self.i].iter().collect();
            match name.as_str() {
                "pi" => return Some(Val::num(std::f64::consts::PI)),
                "e" => return Some(Val::num(std::f64::consts::E)),
                _ => {}
            }
            let f = FUNCS.iter().find(|(n, _)| *n == name)?.1;
            if !self.eat('(') {
                return None;
            }
            let x = self.expr()?;
            return self.eat(')').then_some(Val::num(f(x.v)));
        }
        None
    }
}

fn normalize(src: &str) -> String {
    src.trim()
        .to_lowercase()
        .replace("**", "^")
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            ',' => '.',
            '×' | '·' => '*',
            '÷' | ':' => '/',
            '−' => '-',
            c => c,
        })
        .collect::<String>()
        .replace('√', "sqrt")
        .replace('π', "pi")
}

/// Вычисляет выражение. `None` — это не выражение (обычный поисковый запрос).
pub fn evaluate(src: &str) -> Option<CalcResult> {
    if !looks_like_math(src) {
        return None;
    }
    let s = normalize(src);
    let mut p = Parser {
        s: s.chars().collect(),
        i: 0,
    };
    let r = p.expr()?;
    if p.i != p.s.len() {
        return None;
    }
    let v = if r.pct { r.v / 100.0 } else { r.v };
    if !v.is_finite() {
        return None;
    }
    // Убираем шум плавающей точки (0.1 + 0.2 = 0.30000000000000004 → 0.3).
    let v: f64 = format!("{v:.11e}").parse().ok()?;
    Some(CalcResult {
        value: v,
        display: format_ru(v),
        copy: format_plain(v),
    })
}

/// Быстрая проверка, чтобы не пытаться считать «telegram».
fn looks_like_math(src: &str) -> bool {
    let s = src.to_lowercase();
    let has_digit = s.chars().any(|c| c.is_ascii_digit()) || s.contains("pi") || s.contains('π');
    let has_op =
        s.chars().any(|c| "+-*/^%()×÷√−:".contains(c)) || FUNCS.iter().any(|(n, _)| s.contains(n));
    has_digit && has_op
}

fn format_plain(v: f64) -> String {
    if v != 0.0 && (v.abs() >= 1e15 || v.abs() < 1e-9) {
        return format!("{v:e}");
    }
    let s = format!("{v:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn format_ru(v: f64) -> String {
    let plain = format_plain(v);
    if plain.contains('e') {
        return plain.replace('.', ",");
    }
    let (sign, rest) = plain
        .strip_prefix('-')
        .map_or(("", plain.as_str()), |r| ("-", r));
    let (int, frac) = rest
        .split_once('.')
        .map_or((rest, None), |(i, f)| (i, Some(f)));
    let mut grouped = String::new();
    for (n, c) in int.chars().enumerate() {
        if n > 0 && (int.len() - n) % 3 == 0 {
            grouped.push('\u{00A0}');
        }
        grouped.push(c);
    }
    match frac {
        Some(f) => format!("{sign}{grouped},{f}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(s: &str) -> f64 {
        evaluate(s)
            .unwrap_or_else(|| panic!("не вычислилось: {s}"))
            .value
    }

    #[test]
    fn arithmetic() {
        assert_eq!(val("2 + 2 * 2"), 6.0);
        assert_eq!(val("(2 + 2) * 2"), 8.0);
        assert_eq!(val("2^10"), 1024.0);
        assert_eq!(val("-2^2"), -4.0);
        assert_eq!(val("0.1 + 0.2"), 0.3);
        assert_eq!(val("10 / 4"), 2.5);
        assert_eq!(val("1,5 * 2"), 3.0);
    }

    #[test]
    fn percent() {
        assert_eq!(val("200 + 15%"), 230.0);
        assert_eq!(val("200 - 10%"), 180.0);
        assert_eq!(val("200 * 15%"), 30.0);
        assert_eq!(val("50%"), 0.5);
    }

    #[test]
    fn functions() {
        assert_eq!(val("sqrt(144) * 3 + 15%"), 41.4);
        assert_eq!(val("√(16)"), 4.0);
        assert_eq!(val("abs(-3)"), 3.0);
        assert!((val("sin(pi / 2)") - 1.0).abs() < 1e-12);
        assert_eq!(val("log(1000)"), 3.0);
    }

    #[test]
    fn not_math() {
        assert!(evaluate("telegram").is_none());
        assert!(evaluate("7zip").is_none());
        assert!(evaluate("2 +").is_none());
        assert!(evaluate("1 / 0").is_none());
        assert!(evaluate("foo(2)").is_none());
    }

    #[test]
    fn formatting() {
        let r = evaluate("1234567.5 + 0").unwrap();
        assert_eq!(r.display, "1\u{a0}234\u{a0}567,5");
        assert_eq!(r.copy, "1234567.5");
        assert_eq!(evaluate("-1000 * 1").unwrap().display, "-1\u{a0}000");
    }
}
