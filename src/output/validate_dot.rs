/// Basic structural validation of a DOT file before handing it to Graphviz.
///
/// Catches generation bugs early (unbalanced braces, empty file, wrong
/// keyword) and gives a clear error instead of a cryptic renderer crash.
/// Returns `Ok(())` when the file looks well-formed, `Err(msg)` otherwise.
pub fn validate_dot_file(path: &str) -> Result<(), String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;

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

    // Plain string labels (label="...") must contain only ASCII and must not
    // have unescaped interior quotes.  Non-ASCII or a bare " inside the value
    // produce a syntactically invalid DOT file that can segfault Graphviz.
    check_plain_string_labels(&content, path)?;

    Ok(())
}

/// Scan every `label="..."` value in `content` for non-ASCII characters and
/// unescaped interior `"` characters.
///
/// Returns the first violation found, or `Ok(())` if the file is clean.
fn check_plain_string_labels(content: &str, path: &str) -> Result<(), String> {
    let marker = "label=\"";
    let mut rest = content;
    let mut offset = 0usize;

    while let Some(rel) = rest.find(marker) {
        let abs_start = offset + rel + marker.len();
        rest = &rest[rel + marker.len()..];
        offset = abs_start;

        // Parse the string value, honouring \" escapes.
        let mut prev_backslash = false;
        let mut label_value = String::new();
        let mut closed = false;

        for ch in rest.chars() {
            let ch_len = ch.len_utf8();
            if ch == '"' && !prev_backslash {
                offset += ch_len;
                rest = &rest[ch_len..];
                closed = true;
                break;
            }
            if !ch.is_ascii() {
                return Err(format!(
                    "'{path}': non-ASCII character {:?} inside a plain string label \
                     — use HTML label (label=<...>) with HTML entities for non-ASCII content",
                    ch
                ));
            }
            prev_backslash = ch == '\\' && !prev_backslash;
            label_value.push(ch);
            offset += ch_len;
            rest = &rest[ch_len..];
        }

        if !closed {
            // Structural issue — brace checker will catch this; skip.
            break;
        }

        // After the closing quote, check for unescaped-quote split pattern.
        // If what follows (after whitespace) is a bare word immediately
        // followed by `"` — e.g. `Broken" Sub` — the original string had an
        // unescaped `"` that split the label value.
        let after: &str = rest.trim_start_matches([' ', '\t']);
        let suspicious = after.starts_with('"') || {
            // Word char(s) immediately followed by `"` (no `=` in between)
            let word_end = after
                .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                .unwrap_or(after.len());
            word_end > 0 && after.as_bytes().get(word_end) == Some(&b'"')
        };
        if suspicious {
            return Err(format!(
                "'{path}': unescaped quote inside plain string label \
                 — escape embedded quotes as \\\" (found split near: {:?})",
                label_value
            ));
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
    fn accepts_html_entity_in_html_label() {
        let f = write_tmp("digraph G { a [label=<&#x26A0; safe>]; }");
        assert!(validate_dot_file(f.path().to_str().unwrap()).is_ok());
    }

    // ── plain string label checks ────────────────────────────────────────────

    #[test]
    fn non_ascii_in_plain_string_label_is_detected() {
        // ⚠ in a plain string label (label="...") — not an HTML label
        let f = write_tmp(r#"digraph G { a [label="⚠ warning"]; }"#);
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("non-ASCII") && err.contains("plain"),
            "expected plain-label non-ASCII error, got: {err}"
        );
    }

    #[test]
    fn non_ascii_in_cluster_plain_label_is_detected() {
        // ⚠ in a cluster label= attribute
        let f = write_tmp("digraph G { subgraph cluster_0 { label=\"Island 1 [⚠ missing]\"; } }");
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("non-ASCII") && err.contains("plain"),
            "expected plain-label non-ASCII error, got: {err}"
        );
    }

    #[test]
    fn unescaped_quote_inside_plain_label_is_detected() {
        // A subscription name containing " breaks label="..." syntax
        let f = write_tmp(r#"digraph G { subgraph cluster_0 { label="My "Broken" Sub"; } }"#);
        let err = validate_dot_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("unescaped") || err.contains("quote") || err.contains("plain"),
            "expected plain-label quote error, got: {err}"
        );
    }

    #[test]
    fn escaped_quote_in_plain_label_is_accepted() {
        let f = write_tmp(r#"digraph G { a [label="foo \"bar\""]; }"#);
        assert!(validate_dot_file(f.path().to_str().unwrap()).is_ok());
    }
}
