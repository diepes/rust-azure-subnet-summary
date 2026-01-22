//! Gap finding between subnets.
//!
//! Identifies unused IP address ranges between allocated subnets.

use crate::models::{next_subnet_ipv4, num_az_hosts, Ipv4, Subnet};
use std::net::Ipv4Addr;

/// Represents a row of subnet data for output.
#[derive(Debug)]
pub struct SubnetPrintRow {
    /// Row index (0 for gap subnets).
    pub j: usize,
    /// Gap indicator or subnet source index.
    pub gap: String,
    /// Subnet CIDR notation.
    pub subnet_cidr: String,
    /// Broadcast address.
    pub broadcast: String,
    /// Number of usable Azure hosts.
    pub az_hosts: usize,
    /// Subnet name.
    pub subnet_name: String,
    /// Subscription display name.
    pub subscription_name: String,
    /// VNet CIDR notation.
    pub vnet_cidr: String,
    /// VNet name.
    pub vnet_name: String,
    /// Azure region.
    pub location: String,
    /// NSG name (extracted from full ID).
    pub nsg: String,
    /// DNS servers.
    pub dns: String,
    /// Subscription ID.
    pub subscription_id: String,
    /// Number of IP configurations using this subnet.
    pub ip_configurations_count: u32,
}

/// Process a subnet and generate output rows including any gaps.
///
/// # Arguments
/// * `s` - The subnet to process
/// * `i` - The index of this subnet
/// * `next_ip` - The expected next IP address
/// * `vnet_previous_cidr` - The previous VNet's CIDR
/// * `default_cidr_mask` - Default mask size for gap subnets
/// * `_skip_subnet_smaller_than` - Skip subnets smaller than this (unused)
///
/// # Returns
/// A tuple of (next_ip, vnet_cidr, rows)
#[allow(unused_variables)]
pub fn process_subnet_row(
    s: &Subnet,
    i: usize,
    mut next_ip: Ipv4Addr,
    mut vnet_previous_cidr: Ipv4,
    default_cidr_mask: u8,
    _skip_subnet_smaller_than: Ipv4Addr,
) -> (Ipv4Addr, Ipv4, Vec<SubnetPrintRow>) {
    let mut rows = Vec::new();

    // Handle empty subnet_cidr
    let subnet_cidr = match s.subnet_cidr {
        Some(s_cidr) => s_cidr,
        None => {
            log::warn!(
                "Warning: subnet_cidr is None for subnet_name: {}",
                s.subnet_name
            );
            rows.push(create_row_from_subnet(s, i, "None", "none", "none", 0));
            return (next_ip, vnet_previous_cidr, rows);
        }
    };

    // Look for unused subnet gaps
    assert!(
        next_ip <= subnet_cidr.addr,
        "next_ip[{}] > subnet_cidr[{}] should never happen.",
        next_ip,
        subnet_cidr
    );

    // Create gap subnets
    while next_ip < subnet_cidr.lo() {
        let next_mask = find_biggest_subnet(next_ip, default_cidr_mask, subnet_cidr);
        let next_subnet = Ipv4 {
            addr: next_ip,
            mask: next_mask,
        };

        rows.push(SubnetPrintRow {
            j: 0,
            gap: "-gap-".to_string(),
            subnet_cidr: next_subnet.to_string(),
            broadcast: next_subnet.broadcast().unwrap().addr.to_string(),
            az_hosts: num_az_hosts(next_mask).unwrap() as usize,
            subnet_name: "None".to_string(),
            subscription_name: s.subscription_name.clone(),
            vnet_cidr: format_vnet_cidr(&s.vnet_cidr),
            vnet_name: s.vnet_name.clone(),
            location: "None".to_string(),
            nsg: "Unused_nsg".to_string(),
            dns: "Unused_dns".to_string(),
            subscription_id: s.subscription_id.clone(),
            ip_configurations_count: 0,
        });

        next_ip = next_subnet_ipv4(next_subnet, None).unwrap().lo();
    }

    vnet_previous_cidr = s.vnet_cidr[0];

    // Add the actual subnet row
    rows.push(SubnetPrintRow {
        j: i + 1,
        gap: s
            .gap
            .as_ref()
            .unwrap_or(&format!("Sub{}", s.src_index))
            .to_string(),
        subnet_cidr: subnet_cidr.to_string(),
        broadcast: subnet_cidr.broadcast().unwrap().addr.to_string(),
        az_hosts: num_az_hosts(subnet_cidr.mask).unwrap() as usize,
        subnet_name: s.subnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        vnet_cidr: format_vnet_cidr(&s.vnet_cidr),
        vnet_name: s.vnet_name.clone(),
        location: s.location.clone(),
        nsg: extract_nsg_name(s.nsg.as_deref()),
        dns: format_dns_servers(s.dns_servers.as_deref()),
        subscription_id: s.subscription_id.clone(),
        ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
    });

    next_ip = next_subnet_ipv4(subnet_cidr, None).unwrap().lo();
    (next_ip, vnet_previous_cidr, rows)
}

/// Create a row for a subnet with no CIDR.
fn create_row_from_subnet(
    s: &Subnet,
    i: usize,
    gap: &str,
    cidr: &str,
    broadcast: &str,
    az_hosts: usize,
) -> SubnetPrintRow {
    SubnetPrintRow {
        j: i + 1,
        gap: gap.to_string(),
        subnet_cidr: cidr.to_string(),
        broadcast: broadcast.to_string(),
        az_hosts,
        subnet_name: s.subnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        vnet_cidr: format_vnet_cidr(&s.vnet_cidr),
        vnet_name: s.vnet_name.clone(),
        location: s.location.clone(),
        nsg: extract_nsg_name(s.nsg.as_deref()),
        dns: format_dns_servers(s.dns_servers.as_deref()),
        subscription_id: s.subscription_id.clone(),
        ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
    }
}

/// Format VNet CIDR blocks as a comma-separated string.
fn format_vnet_cidr(cidrs: &[Ipv4]) -> String {
    cidrs
        .iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<String>>()
        .join(",")
}

/// Extract NSG name from full resource ID.
fn extract_nsg_name(nsg: Option<&str>) -> String {
    nsg.unwrap_or("None")
        .split('/')
        .last()
        .unwrap_or("None")
        .to_string()
}

/// Format DNS servers as a comma-separated string.
fn format_dns_servers(dns: Option<&[String]>) -> String {
    dns.map(|servers| servers.join(","))
        .unwrap_or_else(|| "None".to_string())
}

/// Find the biggest subnet that fits before the target subnet.
fn find_biggest_subnet(start_ip: Ipv4Addr, start_mask: u8, below_subnet_cidr: Ipv4) -> u8 {
    assert!(
        start_mask <= 32,
        "start_mask[{}] > 32 should never happen.",
        start_mask
    );

    let mut next_mask = start_mask;
    loop {
        let next_subnet = Ipv4 {
            addr: start_ip,
            mask: next_mask,
        };
        if next_subnet.hi() >= below_subnet_cidr.lo() {
            next_mask += 1;
        } else {
            break;
        }
    }

    assert!(
        next_mask <= 32,
        "next_mask[{}] > 32 should never happen.",
        next_mask
    );
    next_mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_biggest_subnet() {
        let start_ip = Ipv4Addr::new(10, 0, 0, 0);
        let below_subnet_cidr = Ipv4::new("10.0.1.0/24").unwrap();
        assert_eq!(24, find_biggest_subnet(start_ip, 8, below_subnet_cidr));
        assert_eq!(28, find_biggest_subnet(start_ip, 28, below_subnet_cidr));

        let start_ip = Ipv4Addr::new(10, 11, 12, 16);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(20, find_biggest_subnet(start_ip, 8, below_subnet_cidr));

        let start_ip = Ipv4Addr::new(10, 0, 0, 0);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(13, find_biggest_subnet(start_ip, 8, below_subnet_cidr));

        let below_subnet_cidr = Ipv4::new("10.192.0.0/24").unwrap();
        assert_eq!(9, find_biggest_subnet(start_ip, 8, below_subnet_cidr));
        assert_eq!(12, find_biggest_subnet(start_ip, 12, below_subnet_cidr));
    }

    #[test]
    fn test_process_subnet_row_01() {
        let mut result: Subnet = Default::default();
        result.vnet_name = "jenkinsarm-vnet".to_string();
        result.vnet_cidr = vec![Ipv4::new("10.0.0.0/16").unwrap()];
        result.subnet_name = "jenkinsarm-snet".to_string();
        result.subnet_cidr = Some(Ipv4::new("10.0.0.0/24").unwrap());

        let (next_ip, _vnet_previous_cidr, print_rows) = process_subnet_row(
            &result,
            1,
            Ipv4Addr::new(10, 0, 0, 0),
            Ipv4::new("0.0.0.0/24").unwrap(),
            28,
            Ipv4Addr::new(10, 17, 255, 255),
        );

        assert_eq!(result.subnet_name, "jenkinsarm-snet");
        assert_eq!(next_ip.to_string(), "10.0.1.0");
        assert_eq!(print_rows.len(), 1, "Expected 1 row for subnet");
    }
}
