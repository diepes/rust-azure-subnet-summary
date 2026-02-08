//! VNet aggregation and operations.
//!
//! Groups subnets into their parent VNets for reporting.

use crate::azure::Data;
use crate::models::VnetList;
use crate::processing::VnetInfo;
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
/// * `excluded_vnets` - Optional list of excluded VNets to display
pub fn print_vnets(vnets: &VnetList<'_>, excluded_vnets: Option<&[VnetInfo]>) -> Result<(), Box<dyn Error>> {
    let excluded_count = excluded_vnets.map(|v| v.len()).unwrap_or(0);
    log::info!(
        "VNETs: found {} VNETs ({} excluded)",
        vnets.vnets.len() + excluded_count,
        excluded_count
    );

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

    // Print excluded VNets
    if let Some(excluded) = excluded_vnets {
        if !excluded.is_empty() {
            log::warn!("Excluded {} VNets due to overlapping CIDRs:", excluded.len());
            for vnet in excluded {
                println!(
                    "VNET: '{vnet_name}' '{subs}' - {cidrs} \x1b[31m[EXCLUDED - {subnet_count} subnet(s)]\x1b[0m",
                    vnet_name = vnet.vnet_name,
                    subs = vnet.subscription_name,
                    cidrs = vnet
                        .vnet_cidr
                        .iter()
                        .map(|cidr| cidr.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                    subnet_count = vnet.subnet_count
                );
            }
        }
    }

    Ok(())
}
