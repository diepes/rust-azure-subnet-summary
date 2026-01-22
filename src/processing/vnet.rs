//! VNet aggregation and operations.
//!
//! Groups subnets into their parent VNets for reporting.

use crate::azure::Data;
use crate::models::VnetList;
use std::error::Error;

/// Build a VnetList from subnet data.
///
/// # Arguments
/// * `data` - The subnet data to aggregate
///
/// # Returns
/// * `Ok(VnetList)` - Aggregated VNet data
pub fn get_vnets(data: &Data) -> Result<VnetList, Box<dyn Error>> {
    let mut vnets = VnetList::new();

    for subnet in data.data.iter() {
        let key = (&subnet.vnet_name as &str, &subnet.subscription_name as &str);

        if let Some(vnet) = vnets.vnets.get_mut(&key) {
            vnet.add_subnet(subnet);
        } else {
            vnets.add_vnet(subnet);
        }
    }

    Ok(vnets)
}

/// Print VNet summary to stdout.
///
/// # Arguments
/// * `vnets` - The VnetList to print
pub fn print_vnets(vnets: &VnetList<'_>) -> Result<(), Box<dyn Error>> {
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
    }

    Ok(())
}
