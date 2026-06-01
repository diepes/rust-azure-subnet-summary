//! Composite Azure data fetch.
//!
//! Provides [`fetch_azure_data`] — a single call that reads all four Azure data
//! sources (subnets, peering, local-gateways, vWAN) from cache or Azure, logs
//! their cache status, and returns an [`AzureData`] bundle.

use super::{
    azure_cache, local_gateway::LocalGatewayData, peering_graph::PeeringData, vwan_graph::VWanData,
    CacheResult,
};
use crate::azure::graph::Data;
use std::error::Error;
use std::path::Path;

/// Optional per-source cache file overrides.
///
/// All fields default to `None`, which means the standard date-stamped filename
/// (`net_YYYY-MM-DD_cache_<source>.json`) will be used.
#[derive(Debug, Default)]
pub struct FetchConfig {
    /// Override path for the subnet cache file.
    pub subnet_cache: Option<String>,
    /// Override path for the peering cache file.
    pub peering_cache: Option<String>,
    /// Override path for the local-gateway cache file.
    pub local_gateway_cache: Option<String>,
    /// Override path for the vWAN cache file.
    pub vwan_cache: Option<String>,
    /// Directory to write / read default cache files.
    /// When `None`, cache files are written to the current directory.
    pub cache_dir: Option<String>,
}

/// All Azure data fetched in a single call.
pub struct AzureData {
    /// Subnet data (cache result includes status + file path).
    pub subnets: CacheResult<Data>,
    /// Peering edge data.
    pub peering_edges: PeeringData,
    /// Local Network Gateway data.
    pub local_gateways: LocalGatewayData,
    /// Virtual WAN topology data.
    pub vwan: VWanData,
}

/// Fetch all Azure data sources in one call.
///
/// Reads from cache when available; falls back to Azure CLI queries and writes
/// new cache files. Cache status is logged at `info` level inside this function
/// so callers don't need to repeat the log pattern.
///
/// # Errors
/// Returns the first error encountered if any source fails.
pub fn fetch_azure_data(config: &FetchConfig) -> Result<AzureData, Box<dyn Error>> {
    let cache_dir: Option<&Path> = config.cache_dir.as_deref().map(Path::new);

    // ── Subnets ──────────────────────────────────────────────────────────────
    let subnet_result = azure_cache::load::<Data>(config.subnet_cache.as_deref(), cache_dir)?;
    if subnet_result.from_cache {
        log::info!("Subnet data read from cache '{}'", subnet_result.cache_file);
    } else {
        log::info!(
            "Subnet data fetched from Azure (cache '{}')",
            subnet_result.cache_file
        );
    }

    // ── Peering ───────────────────────────────────────────────────────────────
    let peering_result =
        azure_cache::load::<PeeringData>(config.peering_cache.as_deref(), cache_dir)?;
    if peering_result.from_cache {
        log::info!(
            "Peering data read from cache '{}'",
            peering_result.cache_file
        );
    } else {
        log::info!(
            "Peering data fetched from Azure (cache '{}')",
            peering_result.cache_file
        );
    }

    // ── Local Gateways ────────────────────────────────────────────────────────
    let lgw_result =
        azure_cache::load::<LocalGatewayData>(config.local_gateway_cache.as_deref(), cache_dir)?;
    if lgw_result.from_cache {
        log::info!(
            "Local gateway data read from cache '{}'",
            lgw_result.cache_file
        );
    } else {
        log::info!(
            "Local gateway data fetched from Azure (cache '{}')",
            lgw_result.cache_file
        );
    }

    // ── vWAN ──────────────────────────────────────────────────────────────────
    let vwan_result = azure_cache::load::<VWanData>(config.vwan_cache.as_deref(), cache_dir)?;
    if vwan_result.from_cache {
        log::info!("vWAN data read from cache '{}'", vwan_result.cache_file);
    } else {
        log::info!(
            "vWAN data fetched from Azure (cache '{}')",
            vwan_result.cache_file
        );
    }

    Ok(AzureData {
        subnets: subnet_result,
        peering_edges: peering_result.data,
        local_gateways: lgw_result.data,
        vwan: vwan_result.data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> FetchConfig {
        FetchConfig {
            subnet_cache: Some("src/tests/test_data/subnet_test_cache_01.json".to_string()),
            peering_cache: Some("src/tests/test_data/peering_test_cache_01.json".to_string()),
            local_gateway_cache: Some(
                "src/tests/test_data/local_gateway_test_cache_01.json".to_string(),
            ),
            vwan_cache: Some("src/tests/test_data/vwan_test_cache_01.json".to_string()),
            ..FetchConfig::default()
        }
    }

    #[test]
    fn fetch_azure_data_returns_all_four_sources() {
        let data = fetch_azure_data(&test_config()).expect("fetch failed");
        // Subnets from subnet_test_cache_01.json
        assert!(
            !data.subnets.data.data.is_empty(),
            "subnets should be non-empty"
        );
        // Peering edges from peering_test_cache_01.json (3 edges)
        assert_eq!(data.peering_edges.data.len(), 3, "expected 3 peering edges");
        // Local gateways from test fixture (1 entry)
        assert_eq!(
            data.local_gateways.data.len(),
            1,
            "expected 1 local gateway"
        );
        assert_eq!(data.local_gateways.data[0].vnet_name, "test-hub-vnet");
        // vWAN: empty fixture
        assert_eq!(data.vwan.data.len(), 0, "expected 0 vWAN rows");
    }

    #[test]
    fn fetch_azure_data_reports_from_cache_true_when_files_exist() {
        let data = fetch_azure_data(&test_config()).expect("fetch failed");
        assert!(
            data.subnets.from_cache,
            "should have read subnets from cache"
        );
    }

    #[test]
    fn fetch_azure_data_fails_when_subnet_cache_missing() {
        let config = FetchConfig {
            subnet_cache: Some("nonexistent_file.json".to_string()),
            ..FetchConfig::default()
        };
        let result = fetch_azure_data(&config);
        assert!(result.is_err(), "should fail when subnet cache is missing");
    }
}
