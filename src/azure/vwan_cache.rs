//! Cache management for Virtual WAN topology data.

use super::azure_cache::{self, AzureSource, CacheResult};
use super::vwan_graph::{run_vwan_graph, VWanData};
use std::error::Error;

impl AzureSource for VWanData {
    fn file_stem() -> &'static str {
        "vwan"
    }
    fn fetch() -> Result<Self, Box<dyn Error>> {
        run_vwan_graph()
    }
}

/// Read vWAN data from cache, or fetch from Azure if cache doesn't exist.
pub fn read_vwan_cache_with_status(
    cache_file: Option<&str>,
) -> Result<CacheResult<VWanData>, Box<dyn Error>> {
    azure_cache::load(cache_file)
}

/// Read vWAN data from cache, or fetch from Azure if not cached.
pub fn read_vwan_cache(cache_file: Option<&str>) -> Result<VWanData, Box<dyn Error>> {
    Ok(read_vwan_cache_with_status(cache_file)?.data)
}
