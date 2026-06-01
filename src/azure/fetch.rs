//! Composite Azure data fetch.
//!
//! Provides [`fetch_azure_data`] — a single call that reads all four Azure data
//! sources (subnets, peering, local-gateways, vWAN) from cache or Azure, logs
//! their cache status, and returns an [`AzureData`] bundle.

use super::{
    local_gateway::LocalGatewayData,
    local_gateway_cache::read_local_gateway_cache_with_status,
    peering_cache::read_peering_cache_with_status, peering_graph::PeeringData,
    vwan_cache::read_vwan_cache_with_status, vwan_graph::VWanData, CacheResult,
};
use crate::azure::cache::read_subnet_cache_with_status;
use std::error::Error;

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
}

/// All Azure data fetched in a single call.
pub struct AzureData {
    /// Subnet data (cache result includes status + file path).
    pub subnets: CacheResult,
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
    // ── Subnets ──────────────────────────────────────────────────────────────
    let subnet_result = read_subnet_cache_with_status(config.subnet_cache.as_deref())?;
    if subnet_result.from_cache {
        log::info!("Subnet data read from cache '{}'", subnet_result.cache_file);
    } else {
        log::info!(
            "Subnet data fetched from Azure (cache '{}')",
            subnet_result.cache_file
        );
    }

    // ── Peering ───────────────────────────────────────────────────────────────
    let peering_result = read_peering_cache_with_status(config.peering_cache.as_deref())?;
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
    let lgw_result = read_local_gateway_cache_with_status(config.local_gateway_cache.as_deref())?;
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
    let vwan_result = read_vwan_cache_with_status(config.vwan_cache.as_deref())?;
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
