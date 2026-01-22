//! Terminal output utilities.
//!
//! Provides formatting helpers for terminal output.

/// Format a value as a quoted, right-aligned field.
///
/// # Arguments
/// * `value` - The value to format
/// * `width` - The minimum width of the field
///
/// # Returns
/// A quoted, right-aligned string
pub fn format_field<T: ToString>(value: T, width: usize) -> String {
    let value_str = value.to_string();
    let quoted = format!("\"{value_str}\"");
    let quoted_len = quoted.len();

    if quoted_len >= width {
        quoted
    } else {
        format!("{quoted:>width$}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_field_short() {
        assert_eq!(format_field("test", 10), "    \"test\"");
    }

    #[test]
    fn test_format_field_exact() {
        assert_eq!(format_field("test", 6), "\"test\"");
    }

    #[test]
    fn test_format_field_long() {
        assert_eq!(format_field("long_value", 5), "\"long_value\"");
    }

    #[test]
    fn test_format_field_number() {
        assert_eq!(format_field(42, 6), "  \"42\"");
    }
}
