//! Azure subnet data model.

use super::Ipv4;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Selects the VNet_CIDR from `cidrs` whose range contains `subnet_cidr`.
/// Falls back to the first element if none match, or to the sentinel if empty.
fn pick_vnet_cidr(cidrs: &[Ipv4], subnet_cidr: Option<Ipv4>) -> Ipv4 {
    if let Some(sc) = subnet_cidr {
        if let Some(&vc) = cidrs.iter().find(|vc| vc.contains(sc.lo())) {
            return vc;
        }
    }
    cidrs
        .first()
        .copied()
        .unwrap_or_else(|| Ipv4::new("0.0.0.0/0").expect("valid sentinel"))
}

/// Raw deserialization target — vnet_cidr kept as Vec to enable correct CIDR selection.
#[derive(Deserialize)]
struct SubnetRaw {
    vnet_name: String,
    vnet_cidr: Vec<Ipv4>,
    subnet_name: String,
    subnet_cidr: Option<Ipv4>,
    nsg: Option<String>,
    location: String,
    dns_servers: Option<Vec<String>>,
    subscription_id: String,
    subscription_name: String,
    ip_configurations_count: Option<u32>,
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
#[serde(from = "SubnetRaw")]
pub struct Subnet {
    /// Name of the virtual network containing this subnet.
    pub vnet_name: String,
    /// The specific VNet_CIDR (address space) this subnet belongs to.
    /// Serialized as a single-element JSON array to match the Azure cache format.
    #[serde(serialize_with = "serialize_vnet_cidr")]
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
}

impl From<SubnetRaw> for Subnet {
    fn from(raw: SubnetRaw) -> Self {
        let vnet_cidr = pick_vnet_cidr(&raw.vnet_cidr, raw.subnet_cidr);
        Subnet {
            vnet_name: raw.vnet_name,
            vnet_cidr,
            subnet_name: raw.subnet_name,
            subnet_cidr: raw.subnet_cidr,
            nsg: raw.nsg,
            location: raw.location,
            dns_servers: raw.dns_servers,
            subscription_id: raw.subscription_id,
            subscription_name: raw.subscription_name,
            ip_configurations_count: raw.ip_configurations_count,
        }
    }
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
    fn vnet_cidr_selects_containing_cidr_from_multi_element_array() {
        // When vnet_cidr has multiple CIDRs, pick the one that contains the subnet_cidr.
        // subnet_cidr=10.166.32.0/20 belongs to 10.166.32.0/19, NOT 10.176.32.0/19 (first elem).
        let json = r#"{
            "vnet_name": "shared-aue-backoffice-spoke-vnet",
            "vnet_cidr": ["10.176.32.0/19", "10.166.32.0/19"],
            "subnet_name": "aks",
            "subnet_cidr": "10.166.32.0/20",
            "location": "australiaeast",
            "subscription_id": "sub-001",
            "subscription_name": "Test Sub"
        }"#;
        let subnet: Subnet = serde_json::from_str(json).expect("deserialize failed");
        assert_eq!(
            subnet.vnet_cidr,
            Ipv4::new("10.166.32.0/19").unwrap(),
            "Must pick the VNet_CIDR that contains the subnet, not blindly take first"
        );
    }

    #[test]
    fn excluded_by_in_cached_json_is_ignored_on_deserialize() {
        // Old cache files may contain excluded_by — must deserialize without error.
        let json = r#"{
            "vnet_name": "my-vnet",
            "vnet_cidr": ["10.0.0.0/16"],
            "subnet_name": "my-subnet",
            "subnet_cidr": "10.0.1.0/24",
            "location": "eastus",
            "subscription_id": "sub-001",
            "subscription_name": "Test Sub",
            "excluded_by": "winner-vnet"
        }"#;
        // Should not fail even though excluded_by is no longer a field.
        let subnet: Subnet = serde_json::from_str(json).expect("deserialize failed");
        assert_eq!(subnet.vnet_name, "my-vnet");
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
