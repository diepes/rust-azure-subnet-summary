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

pub fn process_subnet_row<'a>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subnet_struct::Subnet;

    #[test]
    fn test_process_subnet_row_01() {
        let mut result: Subnet = Default::default();
        result.vnet_name = "jenkinsarm-vnet".to_string();
        result.vnet_cidr = vec![Ipv4::new("10.0.0.0/16").unwrap()];
        result.subnet_name = "jenkinsarm-snet".to_string();
        result.subnet_cidr = Some(Ipv4::new("10.0.0.0/24").unwrap());
        // test process_subnet_row
        let (next_ip, _vnet_previous_cidr, print_rows) = process_subnet_row(
            &result,
            1,
            Ipv4::new("0.0.0.0/24").unwrap(),
            Ipv4::new("0.0.0.0/24").unwrap(),
            28,
            Ipv4Addr::new(10, 17, 255, 255),
        );
        assert_eq!(
            result.subnet_name, "jenkinsarm-snet",
            "Not expected test subnet name."
        );
        assert_eq!(
            next_ip.to_string(),
            "10.0.1.0/28",
            "result.data[0].subnet_cidr ={:?} \n {:?} \n",
            result.subnet_cidr,
            result,
        );
        assert_eq!(print_rows.len(), 1, "Expected 1 row for subnet 151");
    }
}
