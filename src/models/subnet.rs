//! Azure subnet data model.

use super::Ipv4;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents an Azure subnet with its configuration and metadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Subnet {
    /// Name of the virtual network containing this subnet.
    pub vnet_name: String,
    /// CIDR blocks of the virtual network.
    pub vnet_cidr: Vec<Ipv4>,
    /// Name of the subnet.
    pub subnet_name: String,
    /// CIDR block of the subnet (None if not configured).
    pub subnet_cidr: Option<Ipv4>,
    /// Network Security Group ID (if attached).
    pub nsg: Option<String>,
    /// Azure region location.
    pub location: String,
    /// Custom DNS servers configured on the VNet.
    pub dns_servers: Option<Vec<String>>,
    /// Azure subscription ID.
    pub subscription_id: String,
    /// Azure subscription display name.
    pub subscription_name: String,
    /// Number of IP configurations (NICs) using this subnet.
    pub ip_configurations_count: Option<u32>,
    /// Gap indicator for display purposes.
    pub gap: Option<String>,
    /// Record index from source (for tracking/debugging).
    #[serde(default)]
    pub src_index: usize,
    /// Block ID from paginated graph query results.
    #[serde(default)]
    pub block_id: usize,
    /// Name of the VNet that "won" overlap resolution, if this subnet was excluded.
    /// None means this subnet is active (not excluded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_by: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_by_defaults_to_none_on_deserialize() {
        // JSON without excluded_by field — simulates existing cache files
        let json = r#"{
            "vnet_name": "my-vnet",
            "vnet_cidr": ["10.0.0.0/16"],
            "subnet_name": "my-subnet",
            "subnet_cidr": "10.0.1.0/24",
            "location": "eastus",
            "subscription_id": "sub-001",
            "subscription_name": "Test Sub"
        }"#;
        let subnet: Subnet = serde_json::from_str(json).expect("deserialize failed");
        assert_eq!(subnet.excluded_by, None);
    }

    #[test]
    fn excluded_by_round_trips_through_json() {
        let mut subnet = Subnet::default();
        subnet.excluded_by = Some("winner-vnet".to_string());
        let json = serde_json::to_string(&subnet).unwrap();
        let back: Subnet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.excluded_by, Some("winner-vnet".to_string()));
    }
}

impl Default for Subnet {
    fn default() -> Self {
        Subnet {
            vnet_name: "blank".to_string(),
            vnet_cidr: vec![],
            subnet_name: "".to_string(),
            subnet_cidr: None,
            nsg: None,
            location: "blank".to_string(),
            dns_servers: None,
            subscription_id: "blank".to_string(),
            subscription_name: "blank".to_string(),
            ip_configurations_count: None,
            gap: Some("blank".to_string()),
            src_index: 0,
            block_id: 0,
            excluded_by: None,
        }
    }
}

impl fmt::Display for Subnet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cidr = self
            .subnet_cidr
            .map(|c| c.to_string())
            .unwrap_or_else(|| "None".to_string());
        write!(f, "{}/{} ({})", self.vnet_name, self.subnet_name, cidr)
    }
}
