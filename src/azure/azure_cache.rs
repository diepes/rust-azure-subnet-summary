//! Generic cache layer for all Azure data sources.
//!
//! Defines [`AzureSource`] and the single [`load`] function that implement
//! the cache-or-fetch pattern shared across all four data sources.

use chrono;
use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;
use std::path::Path;

/// Result of loading an Azure data source (from cache or a fresh fetch).
#[derive(Debug)]
pub struct CacheResult<T> {
    pub data: T,
    pub from_cache: bool,
    pub cache_file: String,
}

/// Trait for Azure data types that can be persisted as a local JSON cache.
pub trait AzureSource: DeserializeOwned + Serialize {
    /// Stem used in the date-stamped filename, e.g. `"subnet"` →
    /// `net_YYYY-MM-DD_cache_subnet.json`.
    fn file_stem() -> &'static str;

    /// Fetch fresh data from Azure CLI.
    fn fetch() -> Result<Self, Box<dyn Error>>;
}

/// Load data from a cache file, or fetch from Azure when the file is absent.
///
/// If `cache_file` is `Some`, that exact path is used and an error is returned
/// if the file does not exist. If `None`, a date-stamped filename derived from
/// [`AzureSource::file_stem`] is used.
pub fn load<S: AzureSource>(cache_file: Option<&str>) -> Result<CacheResult<S>, Box<dyn Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);

    let cache_file_path = match cache_file {
        Some(file) => {
            if !Path::new(file).exists() {
                return Err(format!("Cache file does not exist: {file}").into());
            }
            log::info!("Using provided cache file: {file}");
            file.to_string()
        }
        None => format!(
            "net_{}_cache_{}.json",
            now.format("%Y-%m-%d"),
            S::file_stem()
        ),
    };

    let (data, from_cache) = match std::fs::read_to_string(&cache_file_path) {
        Ok(json) => {
            log::info!("Reading from cache file: {cache_file_path}");
            let data: S = serde_json::from_str(&json)
                .map_err(|e| format!("Error parsing cache JSON: {e}"))?;
            (data, true)
        }
        Err(_) => {
            log::warn!("Cache file not found: {cache_file_path}");
            let data = S::fetch()?;
            let json = serde_json::to_string_pretty(&data)
                .map_err(|e| format!("Error serializing JSON: {e}"))?;
            log::warn!("Writing data to cache file: {cache_file_path}");
            std::fs::write(&cache_file_path, &json)
                .map_err(|e| format!("Error writing cache file {cache_file_path}: {e}"))?;
            (data, false)
        }
    };

    Ok(CacheResult {
        data,
        from_cache,
        cache_file: cache_file_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Deserialize, Serialize)]
    struct Stub {
        v: i32,
    }

    impl AzureSource for Stub {
        fn file_stem() -> &'static str {
            "stub"
        }
        fn fetch() -> Result<Self, Box<dyn Error>> {
            unreachable!("fetch should not be called in these tests")
        }
    }

    #[test]
    fn load_reads_file_and_reports_from_cache_true() {
        let path = "/tmp/azure_cache_test_load.json";
        std::fs::write(path, r#"{"v":42}"#).unwrap();

        let result = load::<Stub>(Some(path)).expect("load should succeed");

        assert!(result.from_cache, "should report from_cache = true");
        assert_eq!(result.data, Stub { v: 42 });

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn load_fails_when_explicit_file_missing() {
        let result = load::<Stub>(Some("/tmp/azure_cache_no_such_file_xyz.json"));

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("does not exist"),
            "expected 'does not exist' in error, got: {msg}"
        );
    }
}
