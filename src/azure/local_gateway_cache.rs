//! Cache management for Local Network Gateway data.

use super::local_gateway::{run_local_gateway_graph, LocalGatewayData};
use chrono;
use std::error::Error;
use std::path::Path;

/// Result of reading local gateway data, including cache status.
#[derive(Debug)]
pub struct LocalGatewayCacheResult {
    pub data: LocalGatewayData,
    pub from_cache: bool,
    pub cache_file: String,
}

/// Read local gateway data from cache, or fetch from Azure if cache doesn't exist.
pub fn read_local_gateway_cache_with_status(
    cache_file: Option<&str>,
) -> Result<LocalGatewayCacheResult, Box<dyn Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);

    let cache_file_path = match cache_file {
        Some(file) => {
            if !Path::new(file).exists() {
                return Err(format!("Local gateway cache file does not exist: {file}").into());
            }
            log::info!("Using provided local gateway cache file: {file}");
            file.to_string()
        }
        None => format!("net_{}_cache_local-gateway.json", now.format("%Y-%m-%d")),
    };

    let (data, from_cache) = match std::fs::read_to_string(&cache_file_path) {
        Ok(json) => {
            log::info!("Reading local gateway data from cache: {cache_file_path}");
            let data: LocalGatewayData = serde_json::from_str(&json)
                .map_err(|e| format!("Error parsing local gateway cache JSON: {e}"))?;
            (data, true)
        }
        Err(_) => {
            log::warn!("Local gateway cache not found: {cache_file_path}");
            let data = run_local_gateway_graph()?;
            let json = serde_json::to_string_pretty(&data)
                .map_err(|e| format!("Error serialising local gateway JSON: {e}"))?;
            log::warn!("Writing local gateway cache: {cache_file_path}");
            std::fs::write(&cache_file_path, json)
                .map_err(|e| format!("Error writing local gateway cache {cache_file_path}: {e}"))?;
            (data, false)
        }
    };

    Ok(LocalGatewayCacheResult {
        data,
        from_cache,
        cache_file: cache_file_path,
    })
}

/// Read local gateway data from cache, or fetch from Azure if not cached.
pub fn read_local_gateway_cache(
    cache_file: Option<&str>,
) -> Result<LocalGatewayData, Box<dyn Error>> {
    Ok(read_local_gateway_cache_with_status(cache_file)?.data)
}
