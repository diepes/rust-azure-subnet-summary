//! Azure CLI command execution.
//!
//! Provides utilities for running Azure CLI commands and parsing their output.

use colored::Colorize;
use regex::Regex;
use std::error::Error;
use std::process::Command;
use std::sync::OnceLock;

/// Regex for splitting command strings while preserving quoted substrings.
static COMMAND_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_command_regex() -> &'static Regex {
    COMMAND_REGEX.get_or_init(|| {
        Regex::new(r#"'([^']*)'\s*|\"([^\"]*)\"\s*|([^'\s]*)\s*"#).expect("Invalid Regex")
    })
}

/// Run a shell command and return its stdout.
///
/// The command string is split on spaces, with quoted substrings preserved.
///
/// # Arguments
/// * `cmd` - The command string to execute
///
/// # Returns
/// * `Ok(String)` - The stdout output on success
/// * `Err` - If the command fails or produces too much output
///
/// # Panics
/// * If stdout exceeds 500KB (safety limit)
pub fn run(cmd: &str) -> Result<String, Box<dyn Error>> {
    log::debug!("run({cmd})", cmd = cmd.on_blue());

    let cmds: Vec<&str> = split_and_strip(cmd);
    log::trace!("split cmds={:?}", cmds);

    // Build command and add args
    let mut command = Command::new(cmds[0]);
    for arg in cmds.iter().skip(1) {
        command.arg(arg);
    }

    let output = command.output().map_err(|e| {
        log::error!("Command execution failed: {}", e);
        format!("Failed to execute command: {}", e)
    })?;

    if output.status.success() {
        log::debug!("Success cmd: {cmd}");
        log::debug!("Success output.stdout.len(): {}", output.stdout.len());
        log::debug!("Success output.status.code(): {:?}", output.status.code());

        if output.stdout.len() > 500_000 {
            return Err(format!(
                "Response too large: {} bytes for command: {:?}",
                output.stdout.len(),
                cmds
            )
            .into());
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::trace!(
            "code={code:?}, status={status}\n┎######\nstderr=\n{stderr}\n┖######",
            code = output.status.code(),
            status = output.status,
            stderr = stderr.red()
        );
        log::warn!(
            "{failed} to run {cmd}",
            failed = "failed".on_red(),
            cmd = cmd.on_blue()
        );
        return Err(format!("ERROR running: {stderr}").into());
    }

    let stdout = String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    Ok(stdout)
}

/// Split a command string on spaces, preserving quoted substrings.
fn split_and_strip(input: &str) -> Vec<&str> {
    get_command_regex()
        .find_iter(input)
        .map(|m| m.as_str().trim().trim_matches('\'').trim_matches('"'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_and_strip_complex() {
        let input = "Hello 'World War'  'fail' Rust";
        let expected = vec!["Hello", "World War", "fail", "Rust"];
        assert_eq!(split_and_strip(input), expected);
    }

    #[test]
    fn test_split_and_strip_nospaces() {
        let input = "NoSpacesHere";
        let expected = vec!["NoSpacesHere"];
        assert_eq!(split_and_strip(input), expected);
    }

    #[test]
    fn test_split_and_strip_empty_quotes() {
        let input = "Empty '' Single Quotes";
        let expected = vec!["Empty", "", "Single", "Quotes"];
        assert_eq!(split_and_strip(input), expected);
    }

    #[test]
    fn test_quoted_url() {
        let input = "curl \"https://mysite.com?\\$filter=name eq 'john' and surname eq 'smith'\"";
        let expected = vec![
            "curl",
            "https://mysite.com?\\$filter=name eq 'john' and surname eq 'smith'",
        ];
        assert_eq!(split_and_strip(input), expected);
    }
}
