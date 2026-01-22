//! Cache management for subnet data.
//!
//! Provides caching functionality to avoid repeated Azure Graph API calls.

use super::graph::{run_az_cli_graph, Data};
use chrono;
use std::error::Error;
use std::path::Path;

/// Read subnet data from cache file, or fetch from Azure if cache doesn't exist.
///
/// # Arguments
/// * `cache_file` - Optional path to a specific cache file. If None, uses default naming.
///
/// # Returns
/// * `Ok(Data)` - The subnet data from cache or Azure
/// * `Err` - If cache file specified but doesn't exist, or Azure query fails
pub fn read_subnet_cache(cache_file: Option<&str>) -> Result<Data, Box<dyn Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);

    let cache_file = match cache_file {
        Some(file) => {
            if !Path::new(file).exists() {
                return Err(format!("Cache file does not exist: {file}").into());
            }
            log::info!("Using provided cache file: {file}");
            file.to_string()
        }
        None => format!("subnet_cache_{}.json", now.format("%Y-%m-%d")),
    };

    let data = match std::fs::read_to_string(&cache_file) {
        Ok(json) => {
            log::info!("Reading from cache file: {cache_file}");
            serde_json::from_str(&json).map_err(|e| format!("Error parsing cache JSON: {e}"))?
        }
        Err(_) => {
            log::warn!("Cache file not found: {cache_file}");
            let data = run_az_cli_graph()?;
            log::info!("Parsed JSON data received from Azure CLI");

            let json =
                serde_json::to_string(&data).map_err(|e| format!("Error serializing JSON: {e}"))?;
            log::warn!("Writing data to cache file: {cache_file}");
            std::fs::write(&cache_file, json)
                .map_err(|e| format!("Error writing cache file {cache_file}: {e}"))?;
            data
        }
    };

    Ok(data)
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
