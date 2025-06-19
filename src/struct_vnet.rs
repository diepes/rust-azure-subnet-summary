use std::collections::HashMap;

use crate::ipv4::Ipv4;
use crate::struct_subnet::Subnet;

#[derive(Debug)]
pub struct Vnet<'a> {
    pub vnet_name: &'a str,
    pub vnet_cidr: &'a Vec<Ipv4>,
    pub location: &'a str,
    pub subscription_id: &'a str,
    pub subscription_name: &'a str,
    pub subnets: Vec<&'a Subnet>,
}

impl<'a> Vnet<'a> {
    pub fn new(subnet: &Subnet) -> Vnet {
        Vnet {
            vnet_name: &subnet.vnet_name,
            vnet_cidr: &subnet.vnet_cidr,
            location: &subnet.location,
            subscription_id: &subnet.subscription_id,
            subscription_name: &subnet.subscription_name,
            subnets: vec![&subnet],
        }
    }
    pub fn add_subnet(&mut self, subnet: &'a Subnet) {
        self.subnets.push(subnet);
    }
}

type StrVnet = str;
type StrSubscription = str;
pub struct VnetList<'a> {
    // subnet.vnet_name and subnet.subscription_name
    pub vnets: HashMap<(&'a StrVnet, &'a StrSubscription), Vnet<'a>>,
}

impl<'a> VnetList<'a> {
    pub fn new() -> VnetList<'a> {
        VnetList {
            vnets: HashMap::new(),
        }
    }
    pub fn add_vnet(&mut self, subnet: &'a Subnet) {
        // Check if vnet exists, and panics on duplicate
        if self
            .vnets
            .contains_key(&(&subnet.vnet_name, &subnet.subscription_name))
        {}
        self.vnets.insert(
            (&subnet.vnet_name, &subnet.subscription_name),
            Vnet::new(&subnet),
        );
    }
}
