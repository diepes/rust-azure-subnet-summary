use crate::ipv4::Ipv4;
use crate::subnet_print::SubnetPrintRow;
use std::net::Ipv4Addr;

// recieve previous ip and next subnet, add print rows for gap subnets and given subnet
pub fn process_subnet_row<'a>(
    s: &'a crate::subnet_struct::Subnet,
    i: usize,
    mut next_ip: Ipv4Addr,        // next ip from previous run
    mut vnet_previous_cidr: Ipv4, // vnet cidr from previous run
    default_cidr_mask: u8,
    _skip_subnet_smaller_than: Ipv4Addr,
) -> (Ipv4Addr, Ipv4, Vec<SubnetPrintRow>) {
    let mut rows = Vec::new();
    let subnet_cidr: Ipv4;
    // if empty subnet_cidr return it.
    match s.subnet_cidr {
        Some(s_cidr) => {
            subnet_cidr = s_cidr;
        }
        None => {
            log::warn!(
                "Warning: subnet_cidr is None for subnet_name: {}",
                s.subnet_name
            );
            // Subnet with no CIDR IP, add and return nothing else to add.
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
    assert!(
        next_ip <= subnet_cidr.addr,
        "next_ip[{}] > subnet_cidr[{}] should never happen.",
        next_ip,
        subnet_cidr
    );
    // create new subnets
    while next_ip < subnet_cidr.lo()
    // test if next_ip.addr == subnet_cidr.addr
    {
        // calculate min musk below subnet start
        let next_mask = find_bigest_subnet(next_ip, default_cidr_mask, subnet_cidr);
        let next_subnet = Ipv4 {
            addr: next_ip,
            mask: next_mask,
        };
        // Add gap subnet row
        rows.push(SubnetPrintRow {
            j: 0, // Not a real subnet, so no index
            gap: "-gap-".to_string(),
            subnet_cidr: next_subnet.to_string(),
            broadcast: next_subnet.broadcast().unwrap().addr.to_string(),
            az_hosts: crate::ipv4::num_az_hosts(next_mask).unwrap() as usize,
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
        let _vnet_broadcast_max = if s.vnet_cidr[0] == vnet_previous_cidr {
            s.vnet_cidr[0].broadcast().unwrap()
        } else {
            s.vnet_cidr[0]
        };
        next_ip = crate::ipv4::next_subnet_ipv4(next_subnet, None)
            .unwrap()
            .lo();
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
        broadcast: subnet_cidr.broadcast().unwrap().addr.to_string(),
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
    next_ip = crate::ipv4::next_subnet_ipv4(subnet_cidr, None)
        .unwrap()
        .lo();
    (next_ip, vnet_previous_cidr, rows)
}

fn find_bigest_subnet(start_ip: Ipv4Addr, start_mask: u8, below_subnet_cidr: Ipv4) -> u8 {
    assert!(
        start_mask <= 32,
        "start_mask[{}] > 32 should never happen.",
        start_mask
    );
    let mut next_mask = start_mask;
    let mut next_subnet: Ipv4;
    loop {
        next_subnet = Ipv4 {
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
    use crate::subnet_struct::Subnet;

    #[test]
    fn test_find_bigest_subnet() {
        let start_ip = Ipv4Addr::new(10, 0, 0, 0);
        let below_subnet_cidr = Ipv4::new("10.0.1.0/24").unwrap();
        assert_eq!(24, find_bigest_subnet(start_ip, 8, below_subnet_cidr));
        assert_eq!(28, find_bigest_subnet(start_ip, 28, below_subnet_cidr));
        //
        let start_ip = Ipv4Addr::new(10, 11, 12, 16);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(20, find_bigest_subnet(start_ip, 8, below_subnet_cidr));
        // test for small mask 8
        let start_ip = Ipv4Addr::new(10, 0, 0, 0);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(13, find_bigest_subnet(start_ip, 8, below_subnet_cidr));
        // 
        let below_subnet_cidr = Ipv4::new("10.192.0.0/24").unwrap();
        assert_eq!(9, find_bigest_subnet(start_ip, 8, below_subnet_cidr));
        assert_eq!(12, find_bigest_subnet(start_ip, 12, below_subnet_cidr));
    }

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
            Ipv4Addr::new(10, 0, 0, 0),
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
            "10.0.1.0",
            "result.data[0].subnet_cidr ={:?} \nresult = {:?} \n",
            result.subnet_cidr,
            result,
        );
        assert_eq!(print_rows.len(), 1, "Expected 1 row for subnet 151");
    }
}
