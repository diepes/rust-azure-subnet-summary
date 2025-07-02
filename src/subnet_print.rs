use crate::graph_read_subnet_data;
use crate::ipv4::Ipv4;
use colored::Colorize;
use std::error::Error;
use std::net::Ipv4Addr;

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
    pub ip_configurations_count: u32,
}

fn process_subnet_row<'a>(
    s: &'a crate::subnet_struct::Subnet,
    i: usize,
    mut next_ip: Ipv4,
    mut vnet_previous_cidr: Ipv4,
    default_cidr_mask: u8,
    skip_subnet_smaller_than: Ipv4Addr,
) -> (Ipv4, Ipv4, Vec<SubnetPrintRow>) {
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
                vnet_cidr: s
                    .vnet_cidr
                    .iter()
                    .map(|ip| ip.to_string())
                    .collect::<Vec<String>>()
                    .join(","),
                vnet_name: s.vnet_name.clone(),
                location: s.location.clone(),
                nsg: s
                    .nsg
                    .as_ref()
                    .unwrap_or(&"No_NSG_name".to_string())
                    .split("/")
                    .last()
                    .unwrap()
                    .to_string(),
                dns: s
                    .dns_servers
                    .as_ref()
                    .unwrap_or(&vec!["No_Subnet_IPs".to_string()])
                    .join(","),
                subscription_id: s.subscription_id.clone(),
                ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
            });
            return (next_ip, vnet_previous_cidr, rows);
        }
    }
    // Look for unused subnet gaps
    while next_ip.addr > skip_subnet_smaller_than
        && next_ip.addr < subnet_cidr.addr
        && next_ip < subnet_cidr
        && next_ip >= vnet_previous_cidr
        && crate::ipv4::broadcast_addr_ipv4(next_ip).unwrap()
            < crate::ipv4::broadcast_addr_ipv4(vnet_previous_cidr).unwrap()
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
        // Add gap subnet row
        rows.push(SubnetPrintRow {
            j: 0, // Not a real subnet, so no index
            gap: "-gap-".to_string(),
            subnet_cidr: next_ip.to_string(),
            broadcast: next_ip_broadcast.addr.to_string(),
            az_hosts: crate::ipv4::num_az_hosts(next_ip.mask).unwrap() as usize,
            subnet_name: "None".to_string(),
            subscription_name: s.subscription_name.clone(),
            vnet_cidr: s
                .vnet_cidr
                .iter()
                .map(|ip| ip.to_string())
                .collect::<Vec<String>>()
                .join(","),
            vnet_name: s.vnet_name.clone(),
            location: "None".to_string(),
            nsg: "Unused_nsg".to_string(),
            dns: "Unused_dns".to_string(),
            subscription_id: s.subscription_id.clone(),
            ip_configurations_count: 0,
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
    vnet_previous_cidr = s.vnet_cidr[0];
    rows.push(SubnetPrintRow {
        j: i + 1,
        gap: s
            .gap
            .as_ref()
            .unwrap_or(&format!("Sub{}", s.src_index))
            .to_string(),
        subnet_cidr: subnet_cidr.to_string(),
        broadcast: crate::ipv4::broadcast_addr_ipv4(subnet_cidr)
            .unwrap()
            .addr
            .to_string(),
        az_hosts: crate::ipv4::num_az_hosts(subnet_cidr.mask).unwrap() as usize,
        subnet_name: s.subnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        vnet_cidr: s
            .vnet_cidr
            .iter()
            .map(|ip| ip.to_string())
            .collect::<Vec<String>>()
            .join(","),
        vnet_name: s.vnet_name.clone(),
        location: s.location.clone(),
        nsg: s
            .nsg
            .as_ref()
            .unwrap_or(&"None".to_string())
            .split("/")
            .last()
            .unwrap()
            .to_string(),
        dns: s
            .dns_servers
            .as_ref()
            .unwrap_or(&vec!["None".to_string()])
            .join(","),
        subscription_id: s.subscription_id.clone(),
        ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
    });
    if subnet_cidr.mask < 29 {
        next_ip = crate::ipv4::next_subnet_ipv4(subnet_cidr, Some(28)).unwrap();
    } else {
        next_ip = crate::ipv4::next_subnet_ipv4(subnet_cidr, Some(28)).unwrap();
    }
    (next_ip, vnet_previous_cidr, rows)
}

pub fn f<T: ToString>(value: T, width: usize) -> String {
    let value_str = value.to_string();
    let quoted = format!("\"{}\"", value_str); // Wrap value in quotes
    let quoted_len = quoted.len(); // Length including quotes
    if quoted_len >= width {
        quoted // Return as-is if already wider than or equal to width
    } else {
        // Right-align the quoted string with spaces on the left
        format!("{:>width$}", quoted, width = width)
    }
}

pub async fn subnet_print(
    data: &graph_read_subnet_data::Data,
    gap_cidr_mask: u8,
) -> Result<(), Box<dyn Error>> {
    log::info!(
        "#Start subnet_print() add gap subnets with mask /{}",
        gap_cidr_mask
    );
    log::info!("# Got subnet count = {} == {}", data.count, data.data.len());
    println!(
        r#" "cnt",   "gap",     "subnet_cidr", "broadcast",      "subnet_name",     "subscription_name",           "vnet_cidr",           "vnet_name",               "location",    "nsg",       "dns",       "subscription_id""#
    );
    const SKIP_SUBNET_SMALLER_THAN: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);
    let mut next_ip = Ipv4::new("0.0.0.0/24")?;
    let mut vnet_previous_cidr = Ipv4::new("0.0.0.0/24")?;
    let mut output_rows = Vec::new();
    for (i, s) in data.data.iter().enumerate() {
        let (new_next_ip, new_vnet_previous_cidr, rows) = process_subnet_row(
            s,
            i,
            next_ip,
            vnet_previous_cidr,
            gap_cidr_mask,
            SKIP_SUBNET_SMALLER_THAN,
        );
        next_ip = new_next_ip;
        vnet_previous_cidr = new_vnet_previous_cidr;
        output_rows.extend(rows);
    }
    // print the subnets
    for row in output_rows {
        println!(
            r#"{j},{gap},{subnet_cidr},{host_cnt},{broadcast},{subnet_name},{subscription_name},{vnet_cidr},{vnet_name},{location},{nsg},{dns},{subscription_id}"#,
            j = f(row.j, 6),
            gap = f(row.gap, 8),
            subnet_cidr = f(row.subnet_cidr, 18),
            host_cnt = f(
                format!(
                    "{hosts_used}/{hosts_max}_vms",
                    hosts_used = row.ip_configurations_count,
                    hosts_max = row.az_hosts
                ),
                12
            ),
            broadcast = f(format!("{}_br", row.broadcast), 19),
            subnet_name = f(row.subnet_name,24),
            subscription_name = f(row.subscription_name,21),
            vnet_cidr = f(format!("{}_vnet", row.vnet_cidr),24),
            vnet_name = f(row.vnet_name,30),
            location = f(row.location,16),
            nsg = f(row.nsg,13),
            dns = f(row.dns,13),
            subscription_id = f(row.subscription_id,39),
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
    use super::*;
    use crate::get_sorted_subnets;
    use crate::graph_read_subnet_data::read_subnet_cache;
    // Import de_duplicate_subnets if it is defined in graph_read_subnet_data or another module
    use crate::de_duplicate_subnets::de_duplicate_subnets2;

    #[test]
    fn test_subnet_print_04() {
        //let mut data = gen_cache_data();
        let cache_file = Some("src/tests/test_data/subnet_test_cache_04.json");
        let data_unsorted = read_subnet_cache(cache_file).expect("Error reading subnet cache");
        let data = get_sorted_subnets(cache_file).expect("Error reading subnet cache");
        assert_eq!(
            data_unsorted.data.len(),
            data.data.len(),
            "Expected unsorted and sorted subnets to have the same length"
        );
        assert_eq!(
            data.data.len(),
            180,
            "Expected 180 subnets before de-duplication"
        );
        // Replace default subnet filter list
        let filter = vec![
            "default",
            "pkrsn1ooslfxj77", // Once in data
            "pkrsnsnajtq3h3i", // Not in data
            "pkrsnxocivqofa6", // Not in data
            "orggmcmg",        // Once in data
        ];
        let result_unsorted = de_duplicate_subnets2(data_unsorted, Some(&filter))
            .expect("Failed to de-duplicate subnets");
        assert_eq!(
            result_unsorted.data.len(),
            159,
            "Expected 159 subnets after de-duplication. data_unsorted"
        );
        let result =
            de_duplicate_subnets2(data, Some(&filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            159,
            "Expected 159 subnets after de-duplication. data sorted"
        );
        // Verify this is expected dataset
        assert_eq!(result.data[151].subnet_name, "z-ilt-lab5-snet-adds-01");
        assert_eq!(
            result.data[151].subnet_name,
            result_unsorted.data[151].subnet_name
        );

        // test process_subnet_row
        let (next_ip, _vnet_previous_cidr, print_rows) = process_subnet_row(
            &result.data[0],
            1,
            Ipv4::new("0.0.0.0/24").unwrap(),
            Ipv4::new("0.0.0.0/24").unwrap(),
            28,
            Ipv4Addr::new(10, 17, 255, 255),
        );
        assert_eq!(
            result.data[0].subnet_name, "jenkinsarm-snet",
            "Not expected test subnet name."
        );
        assert_eq!(
            next_ip.to_string(),
            "10.0.1.0/28",
            "result.data[0].subnet_cidr ={:?} \n {:?} \n",
            result.data[0].subnet_cidr,
            result.data[0],
        );
        assert_eq!(print_rows.len(), 1, "Expected 1 row for subnet 151");
    }
}
