use crate::graph_read_subnet_data;
use crate::ipv4::Ipv4;
use colored::Colorize;
use std::error::Error;
use std::net::Ipv4Addr;


struct PrintSubnetsState<'a> {
    i: usize,
    s: &'a crate::struct_subnet::Subnet,
    next_ip: Ipv4,
    vnet_previous_cidr: Ipv4,
}

#[derive(Debug)]
pub struct SubnetPrintRow {
    pub j: usize,
    pub gap: String,
    pub subnet_cidr: String,
    pub broadcast: String,
    pub az_hosts: usize,
    pub subnet_name: String,
    pub subscription_name: String,
    pub vnet_cidr: String,
    pub vnet_name: String,
    pub location: String,
    pub nsg: String,
    pub dns: String,
    pub subscription_id: String,
}

fn process_subnet_row<'a>(
    state: PrintSubnetsState<'a>,
    default_cidr_mask: u8,
    skip_subnet_smaller_than: Ipv4Addr,
) -> (PrintSubnetsState<'a>, Vec<SubnetPrintRow>) {
    let s = state.s;
    let i = state.i;
    let mut next_ip = state.next_ip;
    let mut vnet_previous_cidr = state.vnet_previous_cidr;
    let mut rows = Vec::new();
    let subnet_cidr: Ipv4;
    match s.subnet_cidr {
        Some(s_cidr) => {
            subnet_cidr = s_cidr;
        }
        None => {
            log::warn!(
                "Warning: subnet_cidr is None for subnet_name: {}",
                s.subnet_name
            );
            rows.push(SubnetPrintRow {
                j: i + 1,
                gap: "None".to_string(),
                subnet_cidr: "none".to_string(),
                broadcast: "none".to_string(),
                az_hosts: 0,
                subnet_name: s.subnet_name.clone(),
                subscription_name: s.subscription_name.clone(),
                vnet_cidr: s.vnet_cidr.iter().map(|ip| ip.to_string()).collect::<Vec<String>>().join(","),
                vnet_name: s.vnet_name.clone(),
                location: s.location.clone(),
                nsg: s.nsg.as_ref().unwrap_or(&"None".to_string()).split("/").last().unwrap().to_string(),
                dns: s.dns_servers.as_ref().unwrap_or(&vec!["None".to_string()]).join(","),
                subscription_id: s.subscription_id.clone(),
            });
            return (state, rows);
        }
    }
    while next_ip.addr > skip_subnet_smaller_than
        && next_ip.addr < subnet_cidr.addr
        && next_ip < subnet_cidr
        && next_ip >= vnet_previous_cidr
        && crate::ipv4::broadcast_addr_ipv4(next_ip).unwrap() < crate::ipv4::broadcast_addr_ipv4(vnet_previous_cidr).unwrap()
        && next_ip.addr.octets()[0] == s.vnet_cidr[0].addr.octets()[0]
    {
        let mut next_ip_broadcast = crate::ipv4::broadcast_addr_ipv4(next_ip).unwrap();
        if next_ip_broadcast >= subnet_cidr {
            next_ip.mask = subnet_cidr.mask;
            next_ip_broadcast = crate::ipv4::broadcast_addr_ipv4(next_ip).unwrap();
            if next_ip_broadcast >= subnet_cidr {
                panic!("Gap bigger than subnet, after mask reduction !!! next_ip_broadcast:{:?} subnet:{}  next_ip{}", next_ip_broadcast, subnet_cidr, next_ip)
            }
        }
        rows.push(SubnetPrintRow {
            j: 0, // Not a real subnet, so no index
            gap: "gap".to_string(),
            subnet_cidr: next_ip.to_string(),
            broadcast: next_ip_broadcast.addr.to_string(),
            az_hosts: crate::ipv4::num_az_hosts(next_ip.mask).unwrap() as usize,
            subnet_name: "None".to_string(),
            subscription_name: s.subscription_name.clone(),
            vnet_cidr: s.vnet_cidr.iter().map(|ip| ip.to_string()).collect::<Vec<String>>().join(","),
            vnet_name: s.vnet_name.clone(),
            location: "None".to_string(),
            nsg: "None".to_string(),
            dns: "None".to_string(),
            subscription_id: s.subscription_id.clone(),
        });
        let vnet_broadcast_max = if s.vnet_cidr[0] == vnet_previous_cidr {
            crate::ipv4::broadcast_addr_ipv4(s.vnet_cidr[0]).unwrap()
        } else {
            s.vnet_cidr[0]
        };
        if next_ip_broadcast > vnet_broadcast_max || next_ip_broadcast >= subnet_cidr {
            if next_ip_broadcast >= vnet_broadcast_max {
                log::error!(
                    "next_ip_broadcast[{}] >= vnet_broadcast_max[{}]   ... next_ip:[{}]",
                    next_ip_broadcast,
                    vnet_broadcast_max,
                    next_ip,
                );
            }
            if next_ip_broadcast >= subnet_cidr {
                log::error!(
                    "next_ip_broadcast[{}] >= s.subnet_cidr[{}]... next_ip:[{}]",
                    next_ip_broadcast,
                    subnet_cidr,
                    next_ip,
                );
            }
            panic!("Gap bigger than subnet or vnet !!! next:{:?} vnet:{:?} following_subnet:{:?} previous_vnet: {:?}", next_ip_broadcast, s.vnet_cidr[0], subnet_cidr, vnet_previous_cidr)
        }
        next_ip = crate::ipv4::next_subnet_ipv4(next_ip, Some(default_cidr_mask)).unwrap();
    }
    rows.push(SubnetPrintRow {
        j: i + 1,
        gap: s.gap.as_ref().unwrap_or(&format!("Sub{}", s.src_index)).to_string(),
        subnet_cidr: subnet_cidr.to_string(),
        broadcast: crate::ipv4::broadcast_addr_ipv4(subnet_cidr).unwrap().addr.to_string(),
        az_hosts: crate::ipv4::num_az_hosts(subnet_cidr.mask).unwrap() as usize,
        subnet_name: s.subnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        vnet_cidr: s.vnet_cidr.iter().map(|ip| ip.to_string()).collect::<Vec<String>>().join(","),
        vnet_name: s.vnet_name.clone(),
        location: s.location.clone(),
        nsg: s.nsg.as_ref().unwrap_or(&"None".to_string()).split("/").last().unwrap().to_string(),
        dns: s.dns_servers.as_ref().unwrap_or(&vec!["None".to_string()]).join(","),
        subscription_id: s.subscription_id.clone(),
    });
    vnet_previous_cidr = s.vnet_cidr[0];
    if subnet_cidr.mask < 29 {
        next_ip = crate::ipv4::next_subnet_ipv4(subnet_cidr, Some(28)).unwrap();
    } else {
        next_ip = crate::ipv4::next_subnet_ipv4(subnet_cidr, Some(28)).unwrap();
    }
    (
        PrintSubnetsState {
            i,
            s,
            next_ip,
            vnet_previous_cidr,
        },
        rows,
    )
}

pub async fn print_subnets(data: graph_read_subnet_data::Data) -> Result<(), Box<dyn Error>> {
    log::info!("#Start print_subnets()");
    log::info!("# Got subnet count = {} == {}", data.count, data.data.len());
    println!(
        r#""cnt","gap","subnet_cidr","broadcast","subnet_name","subscription_name","vnet_cidr","vnet_name","location","nsg","dns","subscription_id""#
    );
    const DEFAULT_CIDR_MASK: u8 = 28; // /28 = 11 ips for hosts in Azure. (16-5)
    const SKIP_SUBNET_SMALLER_THAN: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);
    let mut next_ip = Ipv4::new("0.0.0.0/24")?;
    let mut vnet_previous_cidr = Ipv4::new("0.0.0.0/24")?;
    let mut output_rows = Vec::new();
    for (i, s) in data.data.iter().enumerate() {
        let state = PrintSubnetsState {
            i,
            s,
            next_ip,
            vnet_previous_cidr,
        };
        let (new_state, rows) = process_subnet_row(state, DEFAULT_CIDR_MASK, SKIP_SUBNET_SMALLER_THAN);
        next_ip = new_state.next_ip;
        vnet_previous_cidr = new_state.vnet_previous_cidr;
        output_rows.extend(rows);
    }
    // print the subnets
    for row in output_rows {
        println!(
            r#""{j}","{gap}","{subnet_cidr}","{broadcast}({az_hosts}vm)","{subnet_name}","{subscription_name}","{vnet_cidr}","{vnet_name}","{location}","{nsg}","{dns}","{subscription_id}""#,
            j = row.j,
            gap = row.gap,
            subnet_cidr = row.subnet_cidr,
            broadcast = row.broadcast,
            az_hosts = row.az_hosts,
            subnet_name = row.subnet_name,
            subscription_name = row.subscription_name,
            vnet_cidr = row.vnet_cidr,
            vnet_name = row.vnet_name,
            location = row.location,
            nsg = row.nsg,
            dns = row.dns,
            subscription_id = row.subscription_id,
        );
    }
    println!(
        "#{}# End main() Skipped subnet smaller than {:?}",
        "NOTE".on_red(),
        SKIP_SUBNET_SMALLER_THAN
    );
    Ok(())
}


#[cfg(test)]
mod tests {
    use log4rs::filter;

    use super::*;
    use crate::graph_read_subnet_data::read_subnet_cache;
    // Import de_duplicate_subnets if it is defined in graph_read_subnet_data or another module
    use crate::de_duplicate_subnets::de_duplicate_subnets;

        #[test]
    fn test_print_subnets_04() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_04.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            180,
            "Expected 177 subnets before de-duplication"
        );
        // Replace default subnet filter list
        let filter = vec![
            "default",
            "pkrsn1ooslfxj77", // Once in data
            "pkrsnsnajtq3h3i", // Not in data
            "pkrsnxocivqofa6", // Not in data
            "orggmcmg",        // Once in data
        ];
        let result = de_duplicate_subnets(data,Some(filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            173,
            "Expected 173 subnets after de-duplication"
        );
        // Verify this is expected dataset
        assert_eq!(result.data[151].subnet_name, "prod-fax-subnet");

    }
}
