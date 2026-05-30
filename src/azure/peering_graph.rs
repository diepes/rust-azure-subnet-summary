//! Azure Resource Graph query for VNet peering data.

use super::cli;
use crate::config;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// KQL query for fetching VNet peering edges.
///
/// One row per directed peering. VNets with no peerings are excluded (mv-expand drops them).
/// Standalone VNets are inferred from subnet data instead.
const PEERING_QUERY: &str = r#"resources
        | where type == "microsoft.network/virtualnetworks"
        | mv-expand peering=properties.virtualNetworkPeerings
        | project subscription_id=subscriptionId
                ,vnet_name=name
                ,vnet_cidr=properties.addressSpace.addressPrefixes
                ,peering_name=tostring(peering.name)
                ,peering_state=tostring(peering.properties.peeringState)
                ,remote_vnet_id=tostring(peering.properties.remoteVirtualNetwork.id)
        | join kind=leftouter (
            resourcecontainers
                | where type == "microsoft.resources/subscriptions"
                | project subscription_id=subscriptionId, subscription_name=name
            ) on subscription_id
        | project subscription_id, subscription_name, vnet_name, vnet_cidr, peering_name, peering_state, remote_vnet_id
        | sort by vnet_name asc"#;

/// A directed peering edge from one VNet to a remote VNet.
///
/// Azure requires both sides to be configured (A→B and B→A). When both have
/// `peering_state = "Connected"` the logical link is fully active.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PeeringEdge {
    pub subscription_id: String,
    pub subscription_name: String,
    pub vnet_name: String,
    /// VNet address prefixes (display only).
    #[serde(default)]
    pub vnet_cidr: Vec<String>,
    pub peering_name: String,
    pub peering_state: String,
    /// Full ARM resource ID of the remote VNet.
    /// Format: `/subscriptions/{subId}/resourceGroups/{rg}/providers/Microsoft.Network/virtualNetworks/{name}`
    pub remote_vnet_id: String,
}

impl PeeringEdge {
    /// Extract the remote VNet name from the ARM resource ID (last path segment).
    pub fn remote_vnet_name(&self) -> &str {
        self.remote_vnet_id
            .split('/')
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or("")
    }

    /// Extract the remote subscription ID from the ARM resource ID.
    pub fn remote_subscription_id(&self) -> &str {
        let parts: Vec<&str> = self.remote_vnet_id.split('/').collect();
        parts
            .windows(2)
            .find(|w| w[0].eq_ignore_ascii_case("subscriptions"))
            .map(|w| w[1])
            .unwrap_or("")
    }

    /// Returns true if this edge is fully connected.
    pub fn is_connected(&self) -> bool {
        self.peering_state == "Connected"
    }
}

/// Response wrapper for the peering query (mirrors `graph::Data`).
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PeeringData {
    pub data: Vec<PeeringEdge>,
    pub skip_token: Option<String>,
    pub total_records: Option<u32>,
    pub count: i32,
}

/// Execute Azure Resource Graph peering query with automatic pagination.
pub fn run_peering_graph() -> Result<PeeringData, Box<dyn Error>> {
    let mut data: PeeringData = Default::default();
    let mut skip_token_param = String::new();
    let mut count_blocks_returned = 0;

    while skip_token_param != "--skip-token null" {
        let cmd = format!(
            "az graph query --first 50 {skip_token_param} -q '{PEERING_QUERY}' --output json"
        );
        let output = cli::run(&cmd)?;

        let mut json_block_deserializer = serde_json::Deserializer::from_str(&output);
        let json_parsed: PeeringData =
            serde_path_to_error::deserialize(&mut json_block_deserializer).map_err(|e| {
                log::error!("OUTPUT START:\n\n{}\n\nOUTPUT END\n", output);
                format!(
                    "Error parsing peering JSON block {}: path={} error={}",
                    count_blocks_returned,
                    e.path(),
                    e
                )
            })?;

        let skip_token_new = json_parsed
            .skip_token
            .clone()
            .unwrap_or_else(|| "null".to_string());
        let skip_token_new = format!("--skip-token {skip_token_new}");

        if skip_token_new == skip_token_param {
            return Err("Peering skip token not unique - possible infinite loop".into());
        }
        skip_token_param = skip_token_new;

        let count = json_parsed.count;
        data.data.extend(json_parsed.data);
        data.count += count;

        if let Some(block_records) = json_parsed.total_records {
            data.total_records = Some(block_records);
        }

        log::info!(
            "got peering block#{count_blocks_returned:2} record_count=+{count:3} => {total:3} skip_token='{skip_token_param}'",
            total = data.count,
        );

        std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 5));
        count_blocks_returned += 1;
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_vnet_name_from_arm_id() {
        let edge = PeeringEdge {
            remote_vnet_id: "/subscriptions/aaaa-1111/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/spoke-vnet".to_string(),
            ..Default::default()
        };
        assert_eq!(edge.remote_vnet_name(), "spoke-vnet");
    }

    #[test]
    fn remote_subscription_id_from_arm_id() {
        let edge = PeeringEdge {
            remote_vnet_id: "/subscriptions/aaaa-1111/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/spoke-vnet".to_string(),
            ..Default::default()
        };
        assert_eq!(edge.remote_subscription_id(), "aaaa-1111");
    }

    #[test]
    fn remote_vnet_name_empty_for_missing_id() {
        let edge = PeeringEdge::default();
        assert_eq!(edge.remote_vnet_name(), "");
        assert_eq!(edge.remote_subscription_id(), "");
    }

    #[test]
    fn is_connected_true_only_for_connected_state() {
        let connected = PeeringEdge { peering_state: "Connected".to_string(), ..Default::default() };
        let initiated = PeeringEdge { peering_state: "Initiated".to_string(), ..Default::default() };
        assert!(connected.is_connected());
        assert!(!initiated.is_connected());
    }
}
