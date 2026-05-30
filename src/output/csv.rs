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
        // Excluded subnets are not part of the gap-finding pass,
        // but they are emitted at the end as DUP_EXCL_VNET rows.
        if s.excluded_by.is_some() {
            continue;
        }
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

    // Insert DUP_EXCL_VNET rows directly after their winner VNet's last row.
    // Collect groups keyed by winner VNet name, preserving encounter order.
    let mut winner_order: Vec<String> = Vec::new();
    let mut dup_groups: std::collections::HashMap<String, Vec<SubnetPrintRow>> =
        std::collections::HashMap::new();

    for s in data.data.iter().filter(|s| s.excluded_by.is_some()) {
        let winner = s.excluded_by.as_deref().unwrap_or("?").to_string();
        let subnet_cidr_str = s
            .subnet_cidr
            .map(|c| c.to_string())
            .unwrap_or_else(|| "None".to_string());
        let broadcast_str = s
            .subnet_cidr
            .and_then(|c| c.broadcast().ok().map(|b| b.addr.to_string()))
            .unwrap_or_else(|| "None".to_string());
        let az_hosts = s
            .subnet_cidr
            .and_then(|c| crate::models::num_az_hosts(c.mask).ok())
            .unwrap_or(0) as usize;
        let row = SubnetPrintRow {
            j: 0,
            gap: "DUP_EXCL_VNET".to_string(),
            subnet_cidr: subnet_cidr_str,
            broadcast: broadcast_str,
            az_hosts,
            subnet_name: format!("{} [DUP of VNET {}]", s.subnet_name, winner),
            subscription_name: s.subscription_name.clone(),
            vnet_cidr: s
                .vnet_cidr
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(","),
            vnet_name: s.vnet_name.clone(),
            location: s.location.clone(),
            nsg: s
                .nsg
                .as_deref()
                .unwrap_or("None")
                .split('/')
                .next_back()
                .unwrap_or("None")
                .to_string(),
            dns: s
                .dns_servers
                .as_deref()
                .map(|d| d.join(","))
                .unwrap_or_else(|| "None".to_string()),
            subscription_id: s.subscription_id.clone(),
            ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
        };
        if !dup_groups.contains_key(&winner) {
            winner_order.push(winner.clone());
        }
        dup_groups.entry(winner).or_default().push(row);
    }

    // Build (insertion_index, rows) pairs — insert after the last row of each winner VNet.
    // Process from the highest index downward so earlier insertions don't shift later ones.
    let mut insertions: Vec<(usize, Vec<SubnetPrintRow>)> = winner_order
        .into_iter()
        .map(|winner_vnet| {
            let pos = output_rows
                .iter()
                .rposition(|r| r.vnet_name == winner_vnet)
                .map(|i| i + 1)
                .unwrap_or(output_rows.len()); // fallback: append at end
            let rows = dup_groups.remove(&winner_vnet).unwrap_or_default();
            (pos, rows)
        })
        .collect();

    insertions.sort_by(|a, b| b.0.cmp(&a.0));
    for (pos, rows) in insertions {
        let tail = output_rows.split_off(pos);
        output_rows.extend(rows);
        output_rows.extend(tail);
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

    // Also write the duplicates markdown report
    let md_filename = format!("subnets-{}-duplicates.md", date_str);
    super::dup_report::write_duplicates_md(data, &md_filename)?;

    Ok(filename)
}

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

    // Serialize tests that write to the date-based CSV filename to avoid race conditions.
    static CSV_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

    #[test]
    fn excluded_subnet_skipped_in_gap_finder_and_appears_as_dup_in_csv() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};

        fn make_subnet(vnet_name: &str, sub_name: &str, vnet_cidr: &str, subnet_cidr: &str, excluded_by: Option<&str>) -> Subnet {
            let mut s: Subnet = Default::default();
            s.vnet_name = vnet_name.to_string();
            s.subscription_name = sub_name.to_string();
            s.subscription_id = "sub-id".to_string();
            s.vnet_cidr = vec![Ipv4::new(vnet_cidr).unwrap()];
            s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
            s.subnet_name = "my-subnet".to_string();
            s.excluded_by = excluded_by.map(|s| s.to_string());
            s
        }

        // winner processes 10.11.4.0/22 → next_ip becomes 10.11.8.0
        // loser (same IP range) excluded — without skip this panics
        let subnets = vec![
            make_subnet("winner-vnet", "Coretex Production", "10.11.0.0/16", "10.11.4.0/22", None),
            make_subnet("loser-vnet",  "Sandbox",            "10.11.0.0/16", "10.11.4.0/22", Some("winner-vnet")),
        ];
        let data = Data {
            count: subnets.len() as i32,
            skip_token: None,
            total_records: None,
            data: subnets,
        };

        // Must not panic (gap finder skips excluded subnet)
        let path = subnet_print(&data, 28).expect("subnet_print must not panic");
        let contents = std::fs::read_to_string(&path).expect("can read CSV");
        let _ = std::fs::remove_file(&path);

        // Excluded subnet must appear as DUP_EXCL_VNET referencing the winner
        assert!(contents.contains("DUP_EXCL_VNET"), "CSV must contain DUP_EXCL_VNET row");
        assert!(contents.contains("winner-vnet"), "DUP row must reference the winning VNet");
    }

    #[test]
    fn dup_rows_appear_directly_after_winner_vnet_not_at_end() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};

        fn make_subnet(vnet_name: &str, sub_name: &str, vnet_cidr: &str, subnet_cidr: &str, excluded_by: Option<&str>) -> Subnet {
            let mut s: Subnet = Default::default();
            s.vnet_name = vnet_name.to_string();
            s.subscription_name = sub_name.to_string();
            s.subscription_id = "sub-id".to_string();
            s.vnet_cidr = vec![Ipv4::new(vnet_cidr).unwrap()];
            s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
            s.subnet_name = "snet".to_string();
            s.excluded_by = excluded_by.map(|s| s.to_string());
            s
        }

        // winner-vnet: 10.0.0.0/16 with subnet 10.0.0.0/24
        // later-vnet:  10.1.0.0/16 with subnet 10.1.0.0/24 (comes AFTER winner in IP space)
        // loser-vnet:  excluded, DUP of winner-vnet
        //
        // Expected order: winner row → DUP row → later-vnet row
        // Current (broken) order: winner row → later-vnet row → DUP row
        let subnets = vec![
            make_subnet("winner-vnet", "Prod", "10.0.0.0/16", "10.0.0.0/24", None),
            make_subnet("later-vnet",  "Prod", "10.1.0.0/16", "10.1.0.0/24", None),
            make_subnet("loser-vnet",  "Dev",  "10.0.0.0/16", "10.0.0.0/24", Some("winner-vnet")),
        ];
        let data = Data {
            count: subnets.len() as i32,
            skip_token: None,
            total_records: None,
            data: subnets,
        };

        let path = subnet_print(&data, 28).expect("must not panic");
        let contents = std::fs::read_to_string(&path).expect("can read");
        let _ = std::fs::remove_file(&path);

        let dup_pos = contents.find("DUP_EXCL_VNET").expect("DUP_EXCL_VNET must exist");
        let later_pos = contents.find("later-vnet").expect("later-vnet must exist");

        assert!(
            dup_pos < later_pos,
            "DUP row must appear directly after winner-vnet rows, not at the end of the file.\nDUP at byte {dup_pos}, later-vnet at byte {later_pos}"
        );
    }

    #[test]
    fn subnet_print_also_produces_duplicates_md() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};

        fn make_subnet(vnet_name: &str, vnet_cidr: &str, subnet_cidr: &str, excluded_by: Option<&str>) -> Subnet {
            let mut s: Subnet = Default::default();
            s.vnet_name = vnet_name.to_string();
            s.subscription_name = "Sub".to_string();
            s.subscription_id = "sub-id".to_string();
            s.vnet_cidr = vec![Ipv4::new(vnet_cidr).unwrap()];
            s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
            s.subnet_name = "snet".to_string();
            s.excluded_by = excluded_by.map(|s| s.to_string());
            s
        }

        let subnets = vec![
            make_subnet("winner-vnet", "10.0.0.0/16", "10.0.0.0/24", None),
            make_subnet("excl-vnet",   "10.0.0.0/16", "10.0.0.0/24", Some("winner-vnet")),
        ];
        let data = Data { count: 2, skip_token: None, total_records: None, data: subnets };

        let csv_path = subnet_print(&data, 28).expect("must not panic");
        let md_path = csv_path.replace(".csv", "-duplicates.md");
        let _ = std::fs::remove_file(&csv_path);
        assert!(std::fs::metadata(&md_path).is_ok(), "duplicates.md must be created alongside CSV");
        let _ = std::fs::remove_file(&md_path);
    }
}
