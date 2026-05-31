//! Cache management for Virtual WAN topology data.

use super::vwan_graph::{run_vwan_graph, VWanData};
use std::error::Error;
use std::path::Path;

/// Result of reading vWAN data, including cache status.
#[derive(Debug)]
pub struct VWanCacheResult {
    pub data: VWanData,
    pub from_cache: bool,
    pub cache_file: String,
}

/// Read vWAN data from cache, or fetch from Azure if cache doesn't exist.
pub fn read_vwan_cache_with_status(
    cache_file: Option<&str>,
) -> Result<VWanCacheResult, Box<dyn Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);

    let cache_file_path = match cache_file {
        Some(file) => {
            if !Path::new(file).exists() {
                return Err(format!("vWAN cache file does not exist: {file}").into());
            }
            log::info!("Using provided vWAN cache file: {file}");
            file.to_string()
        }
        None => format!("vwan_cache_{}.json", now.format("%Y-%m-%d")),
    };

    let (data, from_cache) = match std::fs::read_to_string(&cache_file_path) {
        Ok(json) => {
            log::info!("Reading vWAN data from cache: {cache_file_path}");
            let data: VWanData = serde_json::from_str(&json)
                .map_err(|e| format!("Error parsing vWAN cache JSON: {e}"))?;
            (data, true)
        }
        Err(_) => {
            log::warn!("vWAN cache not found: {cache_file_path}");
            let data = run_vwan_graph()?;
            let json = serde_json::to_string_pretty(&data)
                .map_err(|e| format!("Error serialising vWAN JSON: {e}"))?;
            log::warn!("Writing vWAN cache: {cache_file_path}");
            std::fs::write(&cache_file_path, json)
                .map_err(|e| format!("Error writing vWAN cache {cache_file_path}: {e}"))?;
            (data, false)
        }
    };

    Ok(VWanCacheResult {
        data,
        from_cache,
        cache_file: cache_file_path,
    })
}

/// Read vWAN data from cache, or fetch from Azure if not cached.
pub fn read_vwan_cache(cache_file: Option<&str>) -> Result<VWanData, Box<dyn Error>> {
    Ok(read_vwan_cache_with_status(cache_file)?.data)
}
