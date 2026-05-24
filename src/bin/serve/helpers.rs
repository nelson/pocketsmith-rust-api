//! Small utility helpers shared by both serve tabs. Anything more
//! specific lives in `transfers/helpers.rs` or `normalise/helpers.rs`.

pub fn format_dollars(cents: i64) -> String {
    let abs_cents = cents.abs();
    let whole = abs_cents / 100;
    let frac = abs_cents % 100;
    let whole_str = whole.to_string();
    let mut result = String::new();
    for (i, c) in whole_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    format!("${formatted}.{frac:02}")
}

pub fn format_short_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 { return date.to_string(); }
    let month: u8 = parts[1].parse().unwrap_or(0);
    let day: u8 = parts[2].parse().unwrap_or(0);
    let month_name = match month {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "???",
    };
    format!("{month_name} {day}")
}

/// Extract a single `key=value` from an HTTP query string.
pub fn extract_param(query: &str, key: &str) -> Option<String> {
    query.split('&')
        .find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next()?;
            let v = parts.next()?;
            if k == key { Some(v.to_string()) } else { None }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_dollars_positive() {
        assert_eq!(format_dollars(1050), "$10.50");
        assert_eq!(format_dollars(100), "$1.00");
        assert_eq!(format_dollars(1), "$0.01");
        assert_eq!(format_dollars(99), "$0.99");
    }

    #[test]
    fn format_dollars_zero() {
        assert_eq!(format_dollars(0), "$0.00");
    }

    #[test]
    fn format_dollars_negative() {
        assert_eq!(format_dollars(-1050), "$10.50");
        assert_eq!(format_dollars(-1), "$0.01");
    }

    #[test]
    fn format_dollars_large_with_commas() {
        assert_eq!(format_dollars(123456789), "$1,234,567.89");
        assert_eq!(format_dollars(100000), "$1,000.00");
        assert_eq!(format_dollars(10000000), "$100,000.00");
    }

    #[test]
    fn format_short_date_valid() {
        assert_eq!(format_short_date("2024-01-15"), "Jan 15");
        assert_eq!(format_short_date("2024-06-01"), "Jun 1");
        assert_eq!(format_short_date("2024-12-31"), "Dec 31");
    }

    #[test]
    fn format_short_date_all_months() {
        let months = [
            ("2024-01-01", "Jan"), ("2024-02-01", "Feb"), ("2024-03-01", "Mar"),
            ("2024-04-01", "Apr"), ("2024-05-01", "May"), ("2024-06-01", "Jun"),
            ("2024-07-01", "Jul"), ("2024-08-01", "Aug"), ("2024-09-01", "Sep"),
            ("2024-10-01", "Oct"), ("2024-11-01", "Nov"), ("2024-12-01", "Dec"),
        ];
        for (input, expected_month) in months {
            let result = format_short_date(input);
            assert!(result.starts_with(expected_month), "expected {expected_month} for {input}, got {result}");
        }
    }

    #[test]
    fn format_short_date_invalid_returns_original() {
        assert_eq!(format_short_date("2024"), "2024");
        assert_eq!(format_short_date(""), "");
    }

    #[test]
    fn format_short_date_non_numeric_parts_produce_fallback() {
        assert_eq!(format_short_date("not-a-date"), "??? 0");
    }

    #[test]
    fn format_short_date_invalid_month_shows_question_marks() {
        assert_eq!(format_short_date("2024-13-01"), "??? 1");
        assert_eq!(format_short_date("2024-00-01"), "??? 1");
    }

    #[test]
    fn extract_param_single() {
        assert_eq!(extract_param("filter=pending", "filter"), Some("pending".to_string()));
    }

    #[test]
    fn extract_param_multiple() {
        assert_eq!(extract_param("filter=pending&conf=high", "filter"), Some("pending".to_string()));
        assert_eq!(extract_param("filter=pending&conf=high", "conf"), Some("high".to_string()));
    }

    #[test]
    fn extract_param_missing_key() {
        assert_eq!(extract_param("filter=pending", "conf"), None);
        assert_eq!(extract_param("", "filter"), None);
    }

    #[test]
    fn extract_param_empty_value() {
        assert_eq!(extract_param("filter=", "filter"), Some("".to_string()));
    }
}
