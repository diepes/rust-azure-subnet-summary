//! CSV output formatting for subnet data.

use crate::azure::Data;
use crate::models::Ipv4;
use crate::processing::{process_subnet_row, SubnetPrintRow};
use colored::Colorize;
use std::error::Error;
use std::net::Ipv4Addr;

use super::terminal::format_field;

/// Print subnet data as CSV to stdout.
///
/// # Arguments
/// * `data` - The subnet data to print
/// * `gap_cidr_mask` - The default CIDR mask for gap subnets
pub fn subnet_print(data: &Data, gap_cidr_mask: u8) -> Result<(), Box<dyn Error>> {
    log::info!(
        "#Start subnet_print() add gap subnets with mask /{}",
        gap_cidr_mask
    );
    log::info!("# Got subnet count = {} == {}", data.count, data.data.len());

    // Print CSV header
    println!(
        r#" "cnt",   "gap",     "subnet_cidr", "broadcast",      "subnet_name",     "subscription_name",           "vnet_cidr",           "vnet_name",               "location",    "nsg",       "dns",       "subscription_id""#
    );

    const SKIP_SUBNET_SMALLER_THAN: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);
    let mut next_ip: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 0);
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

    // Print the subnets as CSV
    for row in output_rows {
        print_csv_row(&row);
    }

    println!(
        "#{}# End main() Skipped subnet smaller than {:?}",
        "NOTE".on_red(),
        SKIP_SUBNET_SMALLER_THAN
    );

    Ok(())
}

/// Print a single CSV row.
fn print_csv_row(row: &SubnetPrintRow) {
    println!(
        r#"{j},{gap},{subnet_cidr},{host_cnt},{broadcast},{subnet_name},{subscription_name},{vnet_cidr},{vnet_name},{location},{nsg},{dns},{subscription_id}"#,
        j = format_field(row.j, 6),
        gap = format_field(&row.gap, 8),
        subnet_cidr = format_field(&row.subnet_cidr, 18),
        host_cnt = format_field(
            format!(
                "{hosts_used}/{hosts_max}_vms",
                hosts_used = row.ip_configurations_count,
                hosts_max = row.az_hosts
            ),
            12
        ),
        broadcast = format_field(format!("{}_br", row.broadcast), 19),
        subnet_name = format_field(&row.subnet_name, 24),
        subscription_name = format_field(&row.subscription_name, 21),
        vnet_cidr = format_field(format!("{}_vnet", row.vnet_cidr), 24),
        vnet_name = format_field(&row.vnet_name, 30),
        location = format_field(&row.location, 16),
        nsg = format_field(&row.nsg, 13),
        dns = format_field(&row.dns, 13),
        subscription_id = format_field(&row.subscription_id, 39),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::read_subnet_cache;
    use crate::get_sorted_subnets;
    use crate::processing::de_duplicate_subnets;

    #[test]
    fn test_subnet_print_04() {
        let cache_file = Some("src/tests/test_data/subnet_test_cache_04.json");
        let data_unsorted = read_subnet_cache(cache_file).expect("Error reading subnet cache");
        let data = get_sorted_subnets(cache_file).expect("Error reading subnet cache");

        assert_eq!(data_unsorted.data.len(), data.data.len());
        assert_eq!(data.data.len(), 180);

        let filter = vec![
            "default",
            "pkrsn1ooslfxj77",
            "pkrsnsnajtq3h3i",
            "pkrsnxocivqofa6",
            "orggmcmg",
        ];

        let result_unsorted = de_duplicate_subnets(data_unsorted, Some(&filter))
            .expect("Failed to de-duplicate subnets");
        assert_eq!(result_unsorted.data.len(), 159);

        let result =
            de_duplicate_subnets(data, Some(&filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(result.data.len(), 159);
        assert_eq!(result.data[151].subnet_name, "z-ilt-lab5-snet-adds-01");

        // Test process_subnet_row
        let (next_ip, _vnet_previous_cidr, print_rows) = process_subnet_row(
            &result.data[0],
            1,
            Ipv4Addr::new(10, 0, 0, 0),
            Ipv4::new("0.0.0.0/24").unwrap(),
            28,
            Ipv4Addr::new(10, 17, 255, 255),
        );

        assert_eq!(result.data[0].subnet_name, "jenkinsarm-snet");
        assert_eq!(next_ip.to_string(), "10.0.1.0");
        assert_eq!(print_rows.len(), 1);
    }
}
