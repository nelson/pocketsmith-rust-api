//! ANSI styling (rule-cli §4.0) + regex syntax highlighting (§14.2).
//! Colour is on for a TTY and off when piped / `--json` / `NO_COLOR`, so
//! scripted and golden output stay deterministic plain text.

use std::io::IsTerminal;

pub(crate) struct Style {
    pub(crate) on: bool,
}

impl Style {
    pub(crate) fn new(json: bool) -> Style {
        let on = !json
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal();
        Style { on }
    }
    pub(crate) fn wrap(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub(crate) fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub(crate) fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub(crate) fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
    pub(crate) fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    pub(crate) fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
}

/// Colourise a regex `pattern` for display. Returns the plain pattern
/// unchanged when colour is off (so it stays a faithful, copyable cell).
///
/// Scheme (rule-cli §14.2):
///   - grouping brackets `( )` → dim grey (structure, de-emphasised)
///   - group constructs `?i` / `?:` / `?P<name>` → blue
///   - every other regex special (`\b \d ^ $ [ ] * + ? { } . |`) → blue
///   - the literal text you're actually matching → **bold green**
pub(crate) fn highlight_regex(style: &Style, pattern: &str) -> String {
    if !style.on {
        return pattern.to_string();
    }
    const GREY: &str = "90";
    const BLUE: &str = "34";
    const LITERAL: &str = "1;32"; // bold green
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Escape sequence: backslash + the following char (e.g. \b \d \*).
            '\\' => {
                let mut esc = String::from('\\');
                if let Some(n) = chars.next() {
                    esc.push(n);
                }
                out.push_str(&style.wrap(BLUE, &esc));
            }
            // Opening bracket: grey. If it begins a group construct
            // (`(?...`), colour the construct prefix blue.
            '(' => {
                out.push_str(&style.wrap(GREY, "("));
                if chars.peek() == Some(&'?') {
                    let mut con = String::from(chars.next().unwrap()); // '?'
                    if matches!(chars.peek(), Some('P') | Some('<')) {
                        // Named group: consume through the closing '>'.
                        while let Some(&n) = chars.peek() {
                            con.push(n);
                            chars.next();
                            if n == '>' {
                                break;
                            }
                        }
                    } else {
                        // Inline flags / non-capturing: flag letters (+ '-'),
                        // optionally terminated by ':' (the ')' is emitted grey
                        // by the loop on the next iteration).
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
                    out.push_str(&style.wrap(BLUE, &con));
                }
            }
            ')' => out.push_str(&style.wrap(GREY, ")")),
            // Every other regex special: blue.
            '^' | '$' | '[' | ']' | '*' | '+' | '?' | '{' | '}' | '.' | '|' => {
                out.push_str(&style.wrap(BLUE, &c.to_string()))
            }
            // The literal text being matched for — bold green.
            other => out.push_str(&style.wrap(LITERAL, &other.to_string())),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Force colour on (bypasses the TTY check) so the scheme is testable.
    fn styled() -> Style {
        Style { on: true }
    }

    #[test]
    fn regex_highlight_colours_brackets_flags_literals_and_specials() {
        let s = styled();
        let out = highlight_regex(&s, r"(?i)ZAP\b");
        // brackets dim grey (90)
        assert!(out.contains("\x1b[90m(\x1b[0m"), "open bracket grey: {out:?}");
        assert!(out.contains("\x1b[90m)\x1b[0m"), "close bracket grey");
        // inline flag ?i blue (34)
        assert!(out.contains("\x1b[34m?i\x1b[0m"), "flags blue");
        // literal letters bold green (1;32)
        assert!(out.contains("\x1b[1;32mZ\x1b[0m"), "literal bold green");
        // escape \b blue
        assert!(out.contains("\x1b[34m\\b\x1b[0m"), "escape blue");
    }

    #[test]
    fn regex_highlight_is_plain_when_colour_off() {
        let s = Style { on: false };
        assert_eq!(highlight_regex(&s, r"(?i)ZAP\b"), r"(?i)ZAP\b");
    }

    #[test]
    fn named_group_prefix_is_blue() {
        let s = styled();
        let out = highlight_regex(&s, r"(?P<account>\d+)");
        assert!(out.contains("\x1b[34m?P<account>\x1b[0m"), "named-group prefix blue: {out:?}");
    }
}
