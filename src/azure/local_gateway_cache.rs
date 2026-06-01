//! Cache management for Local Network Gateway data.

use super::azure_cache::{self, AzureSource, CacheResult};
use super::local_gateway::{run_local_gateway_graph, LocalGatewayData};
use std::error::Error;

impl AzureSource for LocalGatewayData {
    fn file_stem() -> &'static str {
        "local-gateway"
    }
    fn fetch() -> Result<Self, Box<dyn Error>> {
        run_local_gateway_graph()
    }
}

/// Read local gateway data from cache, or fetch from Azure if cache doesn't exist.
pub fn read_local_gateway_cache_with_status(
    cache_file: Option<&str>,
) -> Result<CacheResult<LocalGatewayData>, Box<dyn Error>> {
    azure_cache::load(cache_file, None)
}

/// Read local gateway data from cache, or fetch from Azure if not cached.
pub fn read_local_gateway_cache(
    cache_file: Option<&str>,
) -> Result<LocalGatewayData, Box<dyn Error>> {
    Ok(read_local_gateway_cache_with_status(cache_file)?.data)
}
