use crate::graph_read_subnet_data;
use crate::subnet_add_row;

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
    let mut next_ip: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 0);
    let mut vnet_previous_cidr = Ipv4::new("0.0.0.0/24")?;
    let mut output_rows = Vec::new();
    for (i, s) in data.data.iter().enumerate() {
        // Get list of rows to print up to this subnet
        let (new_next_ip, new_vnet_previous_cidr, rows) = subnet_add_row::process_subnet_row(
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
            subnet_name = f(row.subnet_name, 24),
            subscription_name = f(row.subscription_name, 21),
            vnet_cidr = f(format!("{}_vnet", row.vnet_cidr), 24),
            vnet_name = f(row.vnet_name, 30),
            location = f(row.location, 16),
            nsg = f(row.nsg, 13),
            dns = f(row.dns, 13),
            subscription_id = f(row.subscription_id, 39),
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
    use crate::get_sorted_subnets_legacy as get_sorted_subnets;
    use crate::graph_read_subnet_data::read_subnet_cache;
    use crate::de_duplicate_subnets::de_duplicate_subnets2;
    use crate::subnet_add_row::process_subnet_row;

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
            Ipv4Addr::new(10, 0, 0, 0),
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
            "10.0.1.0",
            "result.data[0].subnet_cidr ={:?} \n {:?} \n",
            result.data[0].subnet_cidr,
            result.data[0],
        );
        assert_eq!(print_rows.len(), 1, "Expected 1 row for subnet 151");
    }
}
