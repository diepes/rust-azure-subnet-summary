//! Cache management for subnet data.
//!
//! Provides caching functionality to avoid repeated Azure Graph API calls.

use super::graph::{run_az_cli_graph, Data};
use chrono;
use std::error::Error;
use std::path::Path;

/// Result of reading subnet data, including cache status.
#[derive(Debug)]
pub struct CacheResult {
    /// The subnet data
    pub data: Data,
    /// Whether data was read from existing cache (true) or freshly fetched (false)
    pub from_cache: bool,
    /// The cache file path used
    pub cache_file: String,
}

/// Read subnet data from cache file, or fetch from Azure if cache doesn't exist.
///
/// # Arguments
/// * `cache_file` - Optional path to a specific cache file. If None, uses default naming.
///
/// # Returns
/// * `Ok(CacheResult)` - The subnet data with cache status info
/// * `Err` - If cache file specified but doesn't exist, or Azure query fails
pub fn read_subnet_cache_with_status(
    cache_file: Option<&str>,
) -> Result<CacheResult, Box<dyn Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);

    let cache_file_path = match cache_file {
        Some(file) => {
            if !Path::new(file).exists() {
                return Err(format!("Cache file does not exist: {file}").into());
            }
            log::info!("Using provided cache file: {file}");
            file.to_string()
        }
        None => format!("subnet_cache_{}.json", now.format("%Y-%m-%d")),
    };

    let (data, from_cache) = match std::fs::read_to_string(&cache_file_path) {
        Ok(json) => {
            log::info!("Reading from cache file: {cache_file_path}");
            let data: Data = serde_json::from_str(&json)
                .map_err(|e| format!("Error parsing cache JSON: {e}"))?;
            (data, true)
        }
        Err(_) => {
            log::warn!("Cache file not found: {cache_file_path}");
            let data = run_az_cli_graph()?;
            log::info!("Parsed JSON data received from Azure CLI");

            let json =
                serde_json::to_string_pretty(&data).map_err(|e| format!("Error serializing JSON: {e}"))?;
            log::warn!("Writing data to cache file: {cache_file_path}");
            std::fs::write(&cache_file_path, json)
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

/// Read subnet data from cache file, or fetch from Azure if cache doesn't exist.
///
/// # Arguments
/// * `cache_file` - Optional path to a specific cache file. If None, uses default naming.
///
/// # Returns
/// * `Ok(Data)` - The subnet data from cache or Azure
/// * `Err` - If cache file specified but doesn't exist, or Azure query fails
pub fn read_subnet_cache(cache_file: Option<&str>) -> Result<Data, Box<dyn Error>> {
    Ok(read_subnet_cache_with_status(cache_file)?.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_subnet_cache() {
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_01.json"))
            .expect("Error reading subnet cache");
        assert!(!data.data.is_empty(), "Data should not be empty");
        assert_eq!(
            data.data[0].vnet_name, "z-env-shared_services-vnet-AbCdEf",
            "Wrong vnet from test sample."
        );
        assert!(data.total_records.is_some(), "Total records should be set");
        assert!(data.count > 0, "Count should be greater than 0");
    }

    #[test]
    fn test_read_subnet_cache_04() {
        let test_cache = "src/tests/test_data/subnet_test_cache_04.json";
        let data = read_subnet_cache(Some(test_cache)).expect("Error reading subnet cache");
        assert!(!data.data.is_empty(), "Data should not be empty");
        assert_eq!(
            data.data.len(),
            180,
            "Expected 180 subnets in test sample {test_cache}"
        );
        assert_eq!(
            data.data[0].vnet_name, "Docker_vSEC",
            "Wrong vnet from test sample."
        );
    }
}
