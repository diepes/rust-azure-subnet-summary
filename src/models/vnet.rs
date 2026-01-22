//! Azure Virtual Network (VNet) data model.

use super::{Ipv4, Subnet};
use std::collections::HashMap;

/// Represents an Azure Virtual Network with its subnets.
#[derive(Debug)]
pub struct Vnet<'a> {
    /// Name of the virtual network.
    pub vnet_name: &'a str,
    /// CIDR blocks of the virtual network.
    pub vnet_cidr: &'a Vec<Ipv4>,
    /// Azure region location.
    pub location: &'a str,
    /// Azure subscription ID.
    pub subscription_id: &'a str,
    /// Azure subscription display name.
    pub subscription_name: &'a str,
    /// Subnets within this VNet.
    pub subnets: Vec<&'a Subnet>,
}

impl<'a> Vnet<'a> {
    /// Create a new VNet from a subnet.
    pub fn new(subnet: &Subnet) -> Vnet {
        Vnet {
            vnet_name: &subnet.vnet_name,
            vnet_cidr: &subnet.vnet_cidr,
            location: &subnet.location,
            subscription_id: &subnet.subscription_id,
            subscription_name: &subnet.subscription_name,
            subnets: vec![subnet],
        }
    }

    /// Add a subnet to this VNet.
    pub fn add_subnet(&mut self, subnet: &'a Subnet) {
        self.subnets.push(subnet);
    }
}

type StrVnet = str;
type StrSubscription = str;

/// Collection of VNets indexed by (vnet_name, subscription_name).
pub struct VnetList<'a> {
    /// HashMap of VNets keyed by (vnet_name, subscription_name).
    pub vnets: HashMap<(&'a StrVnet, &'a StrSubscription), Vnet<'a>>,
}

impl<'a> VnetList<'a> {
    /// Create a new empty VnetList.
    pub fn new() -> VnetList<'a> {
        VnetList {
            vnets: HashMap::new(),
        }
    }

    /// Add a new VNet from a subnet.
    pub fn add_vnet(&mut self, subnet: &'a Subnet) {
        self.vnets.insert(
            (&subnet.vnet_name, &subnet.subscription_name),
            Vnet::new(subnet),
        );
    }
}

impl<'a> Default for VnetList<'a> {
    fn default() -> Self {
        Self::new()
    }
}
