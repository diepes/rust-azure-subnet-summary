use std::collections::HashMap;

use crate::graph_read_subnet_data;
use crate::ipv4::Ipv4;
use crate::subnet_struct::Subnet;

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

impl<'a> Default for VnetList<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> VnetList<'a> {
    pub fn new() -> VnetList<'a> {
        VnetList {
            vnets: HashMap::new(),
        }
    }
    pub fn add_vnet(&mut self, subnet: &'a Subnet) {
        // Check if vnet exists, and panics on duplicate
        self.vnets
            .contains_key(&(&subnet.vnet_name, &subnet.subscription_name));
        self.vnets.insert(
            (&subnet.vnet_name, &subnet.subscription_name),
            Vnet::new(subnet),
        );
    }
    pub fn import_from_subnets(&mut self, subnets: &'a graph_read_subnet_data::Data) {
        for subnet in subnets.data.iter() {
            self.add_vnet(subnet);
        }
    }
}

pub fn get_vnets(
    data: &crate::graph_read_subnet_data::Data,
) -> Result<VnetList, Box<dyn std::error::Error>> {
    let mut vnets = VnetList::new();
    for subnet in data.data.iter() {
        // Check if vnet exists, and panics on duplicate
        if vnets
            .vnets
            .contains_key(&(&subnet.vnet_name, &subnet.subscription_name))
        {
            vnets
                .vnets
                .get_mut(&(&subnet.vnet_name, &subnet.subscription_name))
                .unwrap()
                .add_subnet(subnet);
        } else {
            vnets.add_vnet(subnet);
        }
    }
    Ok(vnets)
}

pub async fn print_vnets(vnets: &VnetList<'_>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("VNETs: found {} VNETs", vnets.vnets.len());
    for ((_vnet_k, subs_k), vnet) in &vnets.vnets {
        println!(
            "VNET: '{vnet_name}' '{subs}' - {cidrs}",
            vnet_name = vnet.vnet_name,
            subs = subs_k,
            cidrs = vnet
                .vnet_cidr
                .iter()
                .map(|cidr| cidr.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );
        // log::info!(
        //     "VNET: {} ({}) - {}",
        //     vnet.vnet_name,
        //     key.0,
        //     vnet.vnet_cidr
        //         .iter()
        //         .map(|cidr| cidr.to_string())
        //         .collect::<Vec<String>>()
        //         .join(", ")
        // );
        // log::info!("  Location: {}", vnet.location);
        // log::info!("  Subscription ID: {}", vnet.subscription_id);
        // log::info!("  Subscription Name: {}", vnet.subscription_name);
        // for subnet in &vnet.subnets {
        //     log::info!(
        //         "  Subnet: {} - CIDR: {:?}",
        //         subnet.subnet_name,
        //         subnet.subnet_cidr
        //     );
        // }
    }
    Ok(())
}
