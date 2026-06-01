//! Generic Azure pagination helper.
//!
//! Drives the skip-token pagination loop common to all Azure Graph query modules.

use std::error::Error;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

/// Minimal page envelope — only the fields pagination needs.
#[derive(Deserialize)]
struct PageEnvelope {
    data: Vec<Value>,
    skip_token: Option<String>,
}

/// Execute a paginated Azure Resource Graph query and return all rows.
///
/// `runner` is called once per page with the full `az` CLI command string and
/// must return the raw JSON output.  In production pass [`crate::azure::cli::run`];
/// in tests pass a closure that returns canned JSON.
///
/// # Errors
/// Returns an error if any page fails to parse or if the skip-token repeats
/// (indicating an infinite loop).
pub(crate) fn paginate<F>(
    query: &str,
    sleep: Duration,
    mut runner: F,
) -> Result<Vec<Value>, Box<dyn Error>>
where
    F: FnMut(&str) -> Result<String, Box<dyn Error>>,
{
    let mut all_rows: Vec<Value> = Vec::new();
    let mut skip_token_param = String::new();

    while skip_token_param != "--skip-token null" {
        let cmd =
            format!("az graph query --first 50 {skip_token_param} -q '{query}' --output json");

        let output = runner(&cmd)?;

        let mut de = serde_json::Deserializer::from_str(&output);
        let page: PageEnvelope = serde_path_to_error::deserialize(&mut de)
            .map_err(|e| format!("Error parsing page JSON: path={} error={}", e.path(), e))?;

        let next_token = page.skip_token.unwrap_or_else(|| "null".to_string());
        let next_token_param = format!("--skip-token {next_token}");

        if next_token_param == skip_token_param {
            return Err("skip token not unique — possible infinite loop".into());
        }

        all_rows.extend(page.data);
        skip_token_param = next_token_param;

        if skip_token_param != "--skip-token null" {
            std::thread::sleep(sleep);
        }
    }

    Ok(all_rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Behavior 1 — single page (skip_token: null) returns all rows
    // ------------------------------------------------------------------
    #[test]
    fn single_page_returns_all_rows() {
        let response = r#"{"data":[{"name":"row1"},{"name":"row2"}],"skip_token":null,"count":2}"#;
        let runner = |_: &str| -> Result<String, Box<dyn Error>> { Ok(response.to_string()) };

        let rows = paginate("SELECT 1", Duration::ZERO, runner).unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "row1");
        assert_eq!(rows[1]["name"], "row2");
    }

    // ------------------------------------------------------------------
    // Behavior 2 — two pages are merged into a single vec
    // ------------------------------------------------------------------
    #[test]
    fn two_pages_are_merged() {
        let page1 = r#"{"data":[{"name":"a"}],"skip_token":"tok1","count":1}"#;
        let page2 = r#"{"data":[{"name":"b"},{"name":"c"}],"skip_token":null,"count":2}"#;

        let responses = std::cell::RefCell::new(vec![page1, page2].into_iter());
        let runner = |_: &str| -> Result<String, Box<dyn Error>> {
            Ok(responses.borrow_mut().next().unwrap().to_string())
        };

        let rows = paginate("SELECT 1", Duration::ZERO, runner).unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["name"], "a");
        assert_eq!(rows[2]["name"], "c");
    }

    // ------------------------------------------------------------------
    // Behavior 3 — duplicate skip token signals an infinite loop
    // ------------------------------------------------------------------
    #[test]
    fn duplicate_skip_token_returns_error() {
        // Both pages return the same non-null token.
        let stuck = r#"{"data":[{"name":"x"}],"skip_token":"stuck","count":1}"#;
        let responses = std::cell::RefCell::new(vec![stuck, stuck].into_iter());
        let runner = |_: &str| -> Result<String, Box<dyn Error>> {
            Ok(responses.borrow_mut().next().unwrap().to_string())
        };

        let err = paginate("SELECT 1", Duration::ZERO, runner).unwrap_err();

        assert!(
            err.to_string().contains("infinite loop"),
            "unexpected error: {err}"
        );
    }
}
