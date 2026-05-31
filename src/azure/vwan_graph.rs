//! Azure Resource Graph query for Virtual WAN hub metadata.
//!
//! Queries `microsoft.network/virtualhubs` — one row per hub — for names,
//! address prefixes, and vWAN associations. Spoke connections are derived
//! separately from `HV_*` peering edges in the peering cache.

use super::cli;
use crate::config;
use serde::{Deserialize, Serialize};
use std::error::Error;

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
    let mut data: VWanData = Default::default();
    let mut skip_token_param = String::new();
    let mut count_blocks = 0;

    while skip_token_param != "--skip-token null" {
        let cmd = format!(
            "az graph query --first 50 {skip_token_param} -q '{VWAN_QUERY}' --output json"
        );
        let output = cli::run(&cmd)?;

        let mut deser = serde_json::Deserializer::from_str(&output);
        let block: VWanData =
            serde_path_to_error::deserialize(&mut deser).map_err(|e| {
                log::error!("OUTPUT START:\n\n{}\n\nOUTPUT END\n", output);
                format!(
                    "Error parsing vWAN JSON block {count_blocks}: path={} error={}",
                    e.path(),
                    e
                )
            })?;

        let skip_token_new = block
            .skip_token
            .clone()
            .unwrap_or_else(|| "null".to_string());
        let skip_token_new = format!("--skip-token {skip_token_new}");
        if skip_token_new == skip_token_param {
            return Err("vWAN skip token not unique - possible infinite loop".into());
        }
        skip_token_param = skip_token_new;

        let count = block.count;
        data.data.extend(block.data);
        data.count += count;
        if let Some(r) = block.total_records {
            data.total_records = Some(r);
        }

        log::info!(
            "got vWAN block#{count_blocks:2} record_count=+{count:3} => {total:3} skip_token='{skip_token_param}'",
            total = data.count,
        );
        std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 5));
        count_blocks += 1;
    }

    Ok(data)
}
