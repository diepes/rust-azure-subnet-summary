//! Azure Resource Graph query for Virtual WAN hub metadata.
//!
//! Queries `microsoft.network/virtualhubs` — one row per hub — for names,
//! address prefixes, and vWAN associations. Spoke connections are derived
//! separately from `HV_*` peering edges in the peering cache.

use super::{cli, paginate::paginate};
use crate::config;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;

/// KQL: one row per vWAN Hub (hub metadata only; spoke connections come from peering cache).
///
/// `hubvirtualnetworkconnections` is a child resource not indexed in ARG — query
/// the parent `virtualhubs` instead to get hub name, CIDR, and vWAN name.
const VWAN_QUERY: &str = r#"resources
    | where type == "microsoft.network/virtualhubs"
    | extend virtual_wan_name = tostring(split(tolower(tostring(properties.virtualWan.id)), "/")[8])
    | join kind=leftouter (
        resourcecontainers
        | where type == "microsoft.resources/subscriptions"
        | project subscription_id = subscriptionId, subscription_name = name
    ) on $left.subscriptionId == $right.subscription_id
    | project subscription_id = subscriptionId
             ,subscription_name
             ,hub_name = name
             ,hub_address_prefix = tostring(properties.addressPrefix)
             ,virtual_wan_name
             ,location
    | sort by hub_name asc"#;

/// One row from the vWAN query: metadata for a single Virtual Hub.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct VWanRow {
    pub subscription_id: String,
    #[serde(default)]
    pub subscription_name: String,
    /// Name of the Virtual Hub (e.g. `p-aue-platform-hub`).
    pub hub_name: String,
    /// Hub address prefix / CIDR (e.g. `10.100.0.0/23`).
    #[serde(default)]
    pub hub_address_prefix: String,
    /// Name of the parent Virtual WAN resource.
    #[serde(default)]
    pub virtual_wan_name: String,
    /// Azure region where the hub is deployed.
    #[serde(default)]
    pub location: String,
}

/// Response wrapper for the vWAN query.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct VWanData {
    pub data: Vec<VWanRow>,
    pub skip_token: Option<String>,
    pub total_records: Option<u32>,
    pub count: i32,
}

/// Execute the Azure Resource Graph vWAN query with automatic pagination.
pub fn run_vwan_graph() -> Result<VWanData, Box<dyn Error>> {
    let sleep = Duration::from_millis(config::SLEEP_MSEC * 5);
    let rows = paginate(VWAN_QUERY, sleep, cli::run)?;

    let data: Vec<VWanRow> = serde_json::from_value(serde_json::Value::Array(rows))
        .map_err(|e| format!("Error parsing vWAN rows: {e}"))?;

    let count = data.len() as i32;
    let total_records = Some(data.len() as u32);

    log::info!("Got {count} vWAN hub rows from az graph query");

    Ok(VWanData {
        data,
        skip_token: None,
        total_records,
        count,
    })
}
