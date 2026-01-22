//! Command execution (legacy module - use azure::cli for new code)
#![allow(dead_code)]

use regex::Regex;
use std::error::Error;

use lazy_static::lazy_static;

use colored::Colorize;
use std::process::Command;

pub fn run(cmd: &str) -> Result<String, Box<dyn Error>> {
    // Use regex to split spaces and keep 'quoted sub' str together.
    log::debug!("run({cmd})", cmd = cmd.on_blue());

    let cmds: Vec<&str> = split_and_strip(cmd);

    log::trace!("split cmds={:?}", cmds);

    // build command and add args
    let mut command = Command::new(cmds[0]);
    for (i, arg) in cmds.iter().enumerate() {
        if i > 0 {
            command.arg(arg);
        }
    }
    let out_result = command.output();
    //    let status = command.status();
    // let status = status.expect("Error getting shell exit status");
    // unwrap result
    let output = match out_result {
        Ok(out) => out,
        Err(e) => {
            log::error!("ERR {}", e);
            panic!("ERR {e}")
        }
    };

    if output.status.success() {
        log::debug!("Success cmd: {cmd}");
        log::debug!("Success output.stdout.len(): {}", output.stdout.len());
        log::debug!("Success output.status.code(): {:?}", output.status.code());
        if output.stdout.len() > 500000 {
            panic! {"Response to much ? len={} cmds=[[{:#?}]]",output.stdout.len(),cmds}
        };
    } else {
        let stderr = String::from_utf8(output.stderr).expect("Error converting utf8");
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

    let stdout = String::from_utf8(output.stdout).expect("Error converting utf8");

    // use std::fs::File;
    // use std::io::Write;
    // let file_name = "cmd_output.txt";
    // let mut file = File::create(file_name).expect("Unable to create file");
    // // Write the string to the file
    // file.write_all(stdout.as_bytes())
    //     .expect("Unable to write to file");
    // log::warn!("output.stdout written to file {} successfully", file_name);
    Ok(stdout)
}

// pub fn string_to_json_vec_map(input: &str) -> Result<Vec<Map<String, Value>>, Box<dyn Error>> {
//     let json_value: serde_json::Value =
//         serde_json::from_str(&input).expect("Parse JsonValue failed");
//     let json_vec: Vec<Map<String, Value>> = match json_value {
//         Value::Array(array) => array
//             .into_iter()
//             .map(|v| v.as_object().expect("Not map to JSON").clone())
//             .collect(),
//         _ => {
//             log::error!("Expected a JSON array");
//             panic!("Expected a JSON array"); // Handle the case where the JSON value is not an array
//         }
//     };
//     Ok(json_vec)
// }

// pub fn string_to_json_vec_string(input: &str) -> Result<Vec<String>, Box<dyn Error>> {
//     let json_value: serde_json::Value =
//         serde_json::from_str(&input).expect("Parse JsonValue failed");
//     let json_vec: Vec<String> = match json_value {
//         Value::Array(array) => array
//             .into_iter()
//             .map(|v| format!("{}", v.as_str().expect("Not string to JSON")))
//             .collect(),
//         _ => {
//             log::error!("Expected a JSON array");
//             panic!("Expected a JSON array"); // Handle the case where the JSON value is not an array
//         }
//     };
//     Ok(json_vec)
// }

fn split_and_strip(input: &str) -> Vec<&str> {
    RE.find_iter(input)
        .map(|m| m.as_str().trim().trim_matches('\'').trim_matches('"'))
        .collect()
}
lazy_static! {
    static ref RE: Regex =
        Regex::new(r#"'([^']*)'\s*|\"([^\"]*)\"\s*|([^'\s]*)\s*"#).expect("Invalid Regex?");
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
        let input2 = "NoSpacesHere";
        let expected2 = vec!["NoSpacesHere"];
        assert_eq!(split_and_strip(input2), expected2);
    }
    #[test]
    fn test_split_and_strip_empty_quotes() {
        let input3 = "Empty '' Single Quotes";
        let expected3 = vec!["Empty", "", "Single", "Quotes"];
        assert_eq!(split_and_strip(input3), expected3);
    }
    #[test]
    fn test_quoted_url() {
        let input3 = "curl \"https://mysite.com?\\$filter=name eq 'john' and surname eq 'smith'\"";
        let expected3 = vec![
            "curl",
            "https://mysite.com?\\$filter=name eq 'john' and surname eq 'smith'",
        ];
        assert_eq!(split_and_strip(input3), expected3);
    }
}
