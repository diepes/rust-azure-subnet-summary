//! Cache management for subnet data.
//!
//! Provides caching functionality to avoid repeated Azure Graph API calls.

use super::azure_cache::{self, AzureSource, CacheResult};
use super::graph::{run_az_cli_graph, Data};
use std::error::Error;

impl AzureSource for Data {
    fn file_stem() -> &'static str {
        "subnet"
    }
    fn fetch() -> Result<Self, Box<dyn Error>> {
        run_az_cli_graph()
    }
}

/// Read subnet data from cache file, or fetch from Azure if cache doesn't exist.
///
/// # Arguments
/// * `cache_file` - Optional path to a specific cache file. If None, uses default naming.
pub fn read_subnet_cache_with_status(
    cache_file: Option<&str>,
) -> Result<CacheResult<Data>, Box<dyn Error>> {
    azure_cache::load(cache_file)
}

/// Read subnet data from cache file, or fetch from Azure if cache doesn't exist.
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
