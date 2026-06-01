//! Cache management for VNet peering data.

use super::azure_cache::{self, AzureSource, CacheResult};
use super::peering_graph::{run_peering_graph, PeeringData};
use std::error::Error;

impl AzureSource for PeeringData {
    fn file_stem() -> &'static str {
        "peering"
    }
    fn fetch() -> Result<Self, Box<dyn Error>> {
        run_peering_graph()
    }
}

/// Read peering data from cache file, or fetch from Azure if cache doesn't exist.
pub fn read_peering_cache_with_status(
    cache_file: Option<&str>,
) -> Result<CacheResult<PeeringData>, Box<dyn Error>> {
    azure_cache::load(cache_file)
}

/// Read peering data from cache, or fetch from Azure if not cached.
pub fn read_peering_cache(cache_file: Option<&str>) -> Result<PeeringData, Box<dyn Error>> {
    Ok(read_peering_cache_with_status(cache_file)?.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_peering_cache_returns_expected_edges() {
        let data = read_peering_cache(Some("src/tests/test_data/peering_test_cache_01.json"))
            .expect("Error reading peering cache");
        assert_eq!(data.count, 3, "Expected 3 peering edges");
        assert_eq!(data.data[0].vnet_name, "broken-vnet");
        assert_eq!(data.data[0].peering_state, "Initiated");
        assert_eq!(data.data[1].vnet_name, "hub-vnet");
        assert_eq!(data.data[1].peering_state, "Connected");
        assert_eq!(data.data[1].remote_vnet_name(), "spoke-vnet");
        assert_eq!(data.data[2].vnet_name, "spoke-vnet");
    }
}
