//! Azure subnet data model.

use super::Ipv4;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Deserializes `vnet_cidr` from a single-element JSON array `["10.0.0.0/16"]`
/// as produced by the Azure Resource Graph cache.
fn deserialize_vnet_cidr<'de, D>(deserializer: D) -> Result<Ipv4, D::Error>
where
    D: Deserializer<'de>,
{
    let v: Vec<Ipv4> = Vec::deserialize(deserializer)?;
    v.into_iter()
        .next()
        .ok_or_else(|| serde::de::Error::custom("vnet_cidr array must not be empty"))
}

/// Serializes `vnet_cidr` back to a single-element JSON array to match the cache format.
fn serialize_vnet_cidr<S>(cidr: &Ipv4, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(1))?;
    seq.serialize_element(cidr)?;
    seq.end()
}

/// Represents an Azure subnet with its configuration and metadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Subnet {
    /// Name of the virtual network containing this subnet.
    pub vnet_name: String,
    /// The specific VNet_CIDR (address space) this subnet belongs to.
    /// Deserialized from / serialized to a single-element JSON array per the Azure cache format.
    #[serde(
        deserialize_with = "deserialize_vnet_cidr",
        serialize_with = "serialize_vnet_cidr"
    )]
    pub vnet_cidr: Ipv4,
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
    fn vnet_cidr_deserializes_from_single_element_array() {
        // Cache JSON has vnet_cidr as a single-element array; field must be plain Ipv4.
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
        // Field is directly an Ipv4 — no indexing required.
        assert_eq!(subnet.vnet_cidr, Ipv4::new("10.0.0.0/16").unwrap());
    }

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
            vnet_cidr: Ipv4::new("0.0.0.0/0").expect("valid sentinel"),
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
