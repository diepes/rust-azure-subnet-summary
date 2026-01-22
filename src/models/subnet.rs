//! Azure subnet data model.

use super::Ipv4;
use serde::{Deserialize, Serialize};

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
        }
    }
}
