//! Cache management for VNet peering data.

use super::peering_graph::{run_peering_graph, PeeringData};
use chrono;
use std::error::Error;
use std::path::Path;

/// Result of reading peering data, including cache status.
#[derive(Debug)]
pub struct PeeringCacheResult {
    pub data: PeeringData,
    pub from_cache: bool,
    pub cache_file: String,
}

/// Read peering data from cache file, or fetch from Azure if cache doesn't exist.
pub fn read_peering_cache_with_status(
    cache_file: Option<&str>,
) -> Result<PeeringCacheResult, Box<dyn Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);

    let cache_file_path = match cache_file {
        Some(file) => {
            if !Path::new(file).exists() {
                return Err(format!("Peering cache file does not exist: {file}").into());
            }
            log::info!("Using provided peering cache file: {file}");
            file.to_string()
        }
        None => format!("peering_cache_{}.json", now.format("%Y-%m-%d")),
    };

    let (data, from_cache) = match std::fs::read_to_string(&cache_file_path) {
        Ok(json) => {
            log::info!("Reading peering from cache: {cache_file_path}");
            let data: PeeringData = serde_json::from_str(&json)
                .map_err(|e| format!("Error parsing peering cache JSON: {e}"))?;
            (data, true)
        }
        Err(_) => {
            log::warn!("Peering cache not found: {cache_file_path}");
            let data = run_peering_graph()?;
            let json = serde_json::to_string_pretty(&data)
                .map_err(|e| format!("Error serialising peering JSON: {e}"))?;
            log::warn!("Writing peering cache: {cache_file_path}");
            std::fs::write(&cache_file_path, json)
                .map_err(|e| format!("Error writing peering cache {cache_file_path}: {e}"))?;
            (data, false)
        }
    };

    Ok(PeeringCacheResult {
        data,
        from_cache,
        cache_file: cache_file_path,
    })
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
        let data =
            read_peering_cache(Some("src/tests/test_data/peering_test_cache_01.json"))
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
