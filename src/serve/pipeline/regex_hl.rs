//! Regex token highlighting for HTML, mirroring the CLI's
//! `highlight_regex` (rule-cli §14.2) so the web rule list + editor show
//! patterns with the same colour vocabulary as `rule list`:
//!   * grouping brackets `( )`            → dim grey (structure)
//!   * group constructs `?i` / `?P<name>` → blue
//!   * other specials `\b \d ^ $ [ ] * + ? { } . |` → blue
//!   * the literal text being matched      → bold green
//!
//! Adjacent characters of the same class coalesce into one span so the
//! markup stays compact.

use maud::{html, Markup};

#[derive(PartialEq, Clone, Copy)]
enum Tok {
    Bracket,
    Meta,
    Literal,
}

impl Tok {
    fn class(self) -> &'static str {
        match self {
            Tok::Bracket => "rx-bracket",
            Tok::Meta => "rx-meta",
            Tok::Literal => "rx-literal",
        }
    }
}

/// Render `pattern` as coloured spans.
pub fn highlight(pattern: &str) -> Markup {
    let toks = tokenize(pattern);
    html! {
        @for (tok, text) in &toks {
            span.(tok.class()) { (text) }
        }
    }
}

/// Tokenise into `(class, run)` pairs, coalescing consecutive same-class
/// characters. Mirrors the CLI scanner.
fn tokenize(pattern: &str) -> Vec<(Tok, String)> {
    let mut out: Vec<(Tok, String)> = Vec::new();
    let mut push = |tok: Tok, s: &str| match out.last_mut() {
        Some((t, run)) if *t == tok => run.push_str(s),
        _ => out.push((tok, s.to_string())),
    };
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut esc = String::from('\\');
                if let Some(n) = chars.next() {
                    esc.push(n);
                }
                push(Tok::Meta, &esc);
            }
            '(' => {
                push(Tok::Bracket, "(");
                if chars.peek() == Some(&'?') {
                    let mut con = String::from(chars.next().unwrap()); // '?'
                    if matches!(chars.peek(), Some('P') | Some('<')) {
                        while let Some(&n) = chars.peek() {
                            con.push(n);
                            chars.next();
                            if n == '>' {
                                break;
                            }
                        }
                    } else {
                        while let Some(&n) = chars.peek() {
                            if n.is_ascii_alphabetic() || n == '-' {
                                con.push(n);
                                chars.next();
                            } else if n == ':' {
                                con.push(n);
                                chars.next();
                                break;
                            } else {
                                break;
                            }
                        }
                    }
                    push(Tok::Meta, &con);
                }
            }
            ')' => push(Tok::Bracket, ")"),
            '^' | '$' | '[' | ']' | '*' | '+' | '?' | '{' | '}' | '.' | '|' => {
                push(Tok::Meta, &c.to_string())
            }
            other => push(Tok::Literal, &other.to_string()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_brackets_flags_literals_and_specials() {
        let h = highlight(r"(?i)ZAP\b").into_string();
        assert!(h.contains("rx-bracket"), "{h}");
        assert!(h.contains("rx-meta"), "{h}");
        assert!(h.contains("rx-literal"), "{h}");
        // The flag construct ?i is one meta run; ZAP is one literal run.
        assert!(h.contains(">?i<") || h.contains("?i"), "{h}");
        assert!(h.contains("ZAP"), "{h}");
    }

    #[test]
    fn coalesces_literal_runs() {
        // "ABC" → a single literal span, not three.
        let h = highlight("ABC").into_string();
        assert_eq!(h.matches("rx-literal").count(), 1, "{h}");
    }

    #[test]
    fn html_special_chars_are_escaped() {
        // A literal '<' must be HTML-escaped by maud, not emitted raw.
        let h = highlight("a<b").into_string();
        assert!(h.contains("&lt;"), "{h}");
    }

    #[test]
    fn named_group_prefix_is_one_meta_run() {
        let h = highlight(r"(?P<account>\d+)").into_string();
        assert!(h.contains("?P&lt;account&gt;") || h.contains("?P<account>"), "{h}");
    }
}
