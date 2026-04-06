//! CSV output formatting for subnet data.

use crate::azure::Data;
use crate::processing::{process_subnet_row, PrevVnetContext, SubnetPrintRow};
use chrono::Local;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::net::Ipv4Addr;

use super::terminal::format_field;

/// Write subnet data as CSV to a file.
///
/// # Arguments
/// * `data` - The subnet data to write
/// * `gap_cidr_mask` - The default CIDR mask for gap subnets
///
/// # Returns
/// The path to the generated CSV file
pub fn subnet_print(data: &Data, gap_cidr_mask: u8) -> Result<String, Box<dyn Error>> {
    log::info!(
        "#Start subnet_print() add gap subnets with mask /{}",
        gap_cidr_mask
    );
    log::info!("# Got subnet count = {} == {}", data.count, data.data.len());

    // Generate filename with current date
    let date_str = Local::now().format("%Y-%m-%d").to_string();
    let filename = format!("subnets-{}.csv", date_str);

    // Open file for writing
    let file = File::create(&filename)?;
    let mut writer = BufWriter::new(file);

    // Write CSV header
    writeln!(
        writer,
        r#" "cnt", "gap"  , "subnet_cidr"    ,"vms"        ,  "broadcast"      , "subnet_name"          ,  "subscription_name",     "vnet_cidr"        ,      "vnet_name","location","nsg","dns","subscription_id""#
    )?;

    const SKIP_SUBNET_SMALLER_THAN: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);
    let mut next_ip: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 0);
    let mut prev_vnet_ctx = PrevVnetContext::default();
    let mut output_rows = Vec::new();

    for (i, s) in data.data.iter().enumerate() {
        let (new_next_ip, new_prev_vnet_ctx, rows) = process_subnet_row(
            s,
            i,
            next_ip,
            prev_vnet_ctx,
            gap_cidr_mask,
            SKIP_SUBNET_SMALLER_THAN,
        );
        next_ip = new_next_ip;
        prev_vnet_ctx = new_prev_vnet_ctx;
        output_rows.extend(rows);
    }

    // Write the subnets as CSV
    for row in &output_rows {
        write_csv_row(&mut writer, row)?;
    }

    writer.flush()?;

    log::info!(
        "Wrote {} rows to '{}' (skipped subnets smaller than {:?})",
        output_rows.len(),
        filename,
        SKIP_SUBNET_SMALLER_THAN
    );

    Ok(filename)
}

/// Write a single CSV row to the writer.
fn write_csv_row<W: Write>(writer: &mut W, row: &SubnetPrintRow) -> Result<(), Box<dyn Error>> {
    writeln!(
        writer,
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
            13
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
    )?;
    Ok(())
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
        let (next_ip, _prev_vnet_ctx, print_rows) = process_subnet_row(
            &result.data[0],
            1,
            Ipv4Addr::new(10, 0, 0, 0),
            PrevVnetContext::default(),
            28,
            Ipv4Addr::new(10, 17, 255, 255),
        );

        assert_eq!(result.data[0].subnet_name, "jenkinsarm-snet");
        assert_eq!(next_ip.to_string(), "10.0.1.0");
        assert_eq!(print_rows.len(), 1);
    }
}
