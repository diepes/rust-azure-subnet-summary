/// Basic structural validation of a DOT file before handing it to Graphviz.
///
/// Catches generation bugs early (unbalanced braces, empty file, wrong
/// keyword) and gives a clear error instead of a cryptic renderer crash.
/// Returns `Ok(())` when the file looks well-formed, `Err(msg)` otherwise.
pub fn validate_dot_file(path: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read '{path}': {e}"))?;

    if content.trim().is_empty() {
        return Err(format!("'{path}' is empty"));
    }

    // Must declare a graph somewhere before the first `{`.
    // Skip leading whitespace and `//` line comments (DOT allows them).
    let mut keyword_found = false;
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("digraph") || t.starts_with("graph") {
            keyword_found = true;
        } else {
            let preview: String = t.chars().take(40).collect();
            return Err(format!(
                "'{path}' does not start with 'digraph' or 'graph' (got: {preview:?})"
            ));
        }
        break;
    }
    if !keyword_found {
        return Err(format!("'{path}' contains only comments / whitespace"));
    }

    // Curly-brace balance (ignores strings / comments, but good enough for
    // catching generation errors like an extra/missing closing brace)
    let (mut depth, mut in_str, mut prev) = (0i32, false, '\0');
    for ch in content.chars() {
        if in_str {
            if ch == '"' && prev != '\\' {
                in_str = false;
            }
        } else {
            match ch {
                '"' => in_str = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(format!(
                            "'{path}' has an unexpected '}}'  (too many closing braces)"
                        ));
                    }
                }
                _ => {}
            }
        }
        prev = ch;
    }
    if depth != 0 {
        return Err(format!(
            "'{path}' has unbalanced braces (net open count: {depth})"
        ));
    }

    // Non-ASCII characters inside HTML labels (label=<...>) crash Graphviz
    // parsers even on recent versions.  Use HTML entities instead (e.g.
    // &#x26A0; for ⚠).  Plain string labels ("...") are unaffected.
    for (lineno, line) in content.lines().enumerate() {
        if line.contains("label=<") {
            let non_ascii: String = line.chars().filter(|c| !c.is_ascii()).take(5).collect();
            if !non_ascii.is_empty() {
                return Err(format!(
                    "'{path}' line {}: non-ASCII characters {:?} inside an HTML label \
                     — use HTML entities instead (e.g. &#x26A0; for ⚠)",
                    lineno + 1,
                    non_ascii
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    #[test]
    fn accepts_valid_digraph() {
        let f = write_tmp("digraph G { a -> b; }");
        assert!(validate_dot_file(f.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn accepts_digraph_with_leading_comments() {
        let f = write_tmp("// header\n// more\ndigraph G { }");
        assert!(validate_dot_file(f.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn rejects_empty_file() {
        let f = write_tmp("   \n  ");
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn rejects_wrong_keyword() {
        let f = write_tmp("not_a_graph { }");
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("does not start with"));
    }

    #[test]
    fn rejects_unbalanced_open_brace() {
        let f = write_tmp("digraph G { a -> b; ");
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("unbalanced"));
    }

    #[test]
    fn rejects_unbalanced_close_brace() {
        let f = write_tmp("digraph G { } }");
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("too many closing braces"));
    }

    #[test]
    fn rejects_non_ascii_in_html_label() {
        let f = write_tmp("digraph G { a [label=<⚠ bad>]; }");
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("non-ASCII"));
        assert!(err.contains("HTML label"));
    }

    #[test]
    fn accepts_non_ascii_in_plain_label() {
        let f = write_tmp(r#"digraph G { a [label="⚠ ok in plain"]; }"#);
        assert!(validate_dot_file(f.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn accepts_html_entity_in_html_label() {
        let f = write_tmp("digraph G { a [label=<&#x26A0; safe>]; }");
        assert!(validate_dot_file(f.path().to_str().unwrap()).is_ok());
    }
}
