//! CSV output formatting for subnet data.

use crate::azure::{Data, VWanRow};
use crate::models::{num_az_hosts, Subnet};
use crate::processing::gap_finder::{extract_nsg_name, format_dns_servers};
use crate::processing::{gaps, ExcludedSubnet, GapKind, SubnetPrintRow, VnetCidr};
use chrono::Local;
use std::cmp::Reverse;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::net::Ipv4Addr;
use std::path::Path;

use super::terminal::format_field;

/// Build the ordered flat list of [`SubnetPrintRow`]s for all subnets.
///
/// Produces gap rows, inserts excluded-subnet (`DUP_EXCL_VNET`) rows after
/// their winner VNet, and inserts vWAN hub rows at the correct sorted position.
/// The result is ready to be written to CSV by [`subnet_print`].
pub fn build_rows(
    subnets: &[Subnet],
    excluded: &[ExcludedSubnet],
    gap_cidr_mask: u8,
    vwan: &[VWanRow],
) -> Vec<SubnetPrintRow> {
    // ── 1. Group subnets into VnetCidr objects ──────────────────────────────
    let mut vnet_cidrs: Vec<VnetCidr> = Vec::new();
    for subnet in subnets {
        if let Some(last) = vnet_cidrs.last_mut() {
            if last.cidr == subnet.vnet_cidr {
                last.subnets.push(subnet.clone());
                continue;
            }
        }
        vnet_cidrs.push(VnetCidr {
            cidr: subnet.vnet_cidr,
            vnet_name: subnet.vnet_name.clone(),
            subscription_id: subnet.subscription_id.clone(),
            subscription_name: subnet.subscription_name.clone(),
            location: subnet.location.clone(),
            subnets: vec![subnet.clone()],
        });
    }

    // ── 2. Map GapEvent → SubnetPrintRow ────────────────────────────────────
    let gap_events = gaps(&vnet_cidrs, gap_cidr_mask);
    let mut output_rows: Vec<SubnetPrintRow> = Vec::with_capacity(gap_events.len());
    let mut subnet_index: usize = 0;

    for event in &gap_events {
        let row = match &event.kind {
            GapKind::Subnet(subnet) => {
                subnet_index += 1;
                let sub_cidr = event.cidr;
                let gateway = subnet.subnet_name == "GatewaySubnet";
                SubnetPrintRow {
                    j: subnet_index,
                    gap: if gateway { "GATEWAY" } else { "" }.to_string(),
                    subnet_cidr: sub_cidr.to_string(),
                    broadcast: sub_cidr.broadcast().unwrap().addr.to_string(),
                    az_hosts: num_az_hosts(sub_cidr.mask).unwrap_or(0) as usize,
                    subnet_name: subnet.subnet_name.clone(),
                    subscription_name: subnet.subscription_name.clone(),
                    vnet_cidr: subnet.vnet_cidr.to_string(),
                    vnet_name: subnet.vnet_name.clone(),
                    location: subnet.location.clone(),
                    nsg: extract_nsg_name(subnet.nsg.as_deref()),
                    dns: format_dns_servers(subnet.dns_servers.as_deref()),
                    subscription_id: subnet.subscription_id.clone(),
                    ip_configurations_count: subnet.ip_configurations_count.unwrap_or(0),
                }
            }
            GapKind::Vnet(vc) => SubnetPrintRow {
                j: 0,
                gap: "-vgap-".to_string(),
                subnet_cidr: event.cidr.to_string(),
                broadcast: event.cidr.broadcast().unwrap().addr.to_string(),
                az_hosts: num_az_hosts(event.cidr.mask).unwrap_or(0) as usize,
                subnet_name: "None".to_string(),
                subscription_name: vc.subscription_name.clone(),
                vnet_cidr: vc.cidr.to_string(),
                vnet_name: vc.vnet_name.clone(),
                location: "None".to_string(),
                nsg: "Unused_nsg".to_string(),
                dns: "Unused_dns".to_string(),
                subscription_id: vc.subscription_id.clone(),
                ip_configurations_count: 0,
            },
            GapKind::Gap => SubnetPrintRow {
                j: 0,
                gap: "-gap-".to_string(),
                subnet_cidr: event.cidr.to_string(),
                broadcast: event.cidr.broadcast().unwrap().addr.to_string(),
                az_hosts: num_az_hosts(event.cidr.mask).unwrap_or(0) as usize,
                subnet_name: "None".to_string(),
                subscription_name: "None".to_string(),
                vnet_cidr: "None".to_string(),
                vnet_name: "None".to_string(),
                location: "None".to_string(),
                nsg: "Unused_nsg".to_string(),
                dns: "Unused_dns".to_string(),
                subscription_id: "None".to_string(),
                ip_configurations_count: 0,
            },
        };
        output_rows.push(row);
    }

    // ── 3. Insert DUP_EXCL_VNET rows after their winner VNet ────────────────
    let mut winner_order: Vec<String> = Vec::new();
    let mut dup_groups: std::collections::HashMap<String, Vec<SubnetPrintRow>> =
        std::collections::HashMap::new();

    for e in excluded {
        let winner = e.winner_vnet_name.clone();
        let s = &e.subnet;
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
            .and_then(|c| num_az_hosts(c.mask).ok())
            .unwrap_or(0) as usize;
        let row = SubnetPrintRow {
            j: 0,
            gap: "DUP_EXCL_VNET".to_string(),
            subnet_cidr: subnet_cidr_str,
            broadcast: broadcast_str,
            az_hosts,
            subnet_name: format!("{} [DUP of VNET {}]", s.subnet_name, winner),
            subscription_name: s.subscription_name.clone(),
            vnet_cidr: s.vnet_cidr.to_string(),
            vnet_name: s.vnet_name.clone(),
            location: s.location.clone(),
            nsg: extract_nsg_name(s.nsg.as_deref()),
            dns: format_dns_servers(s.dns_servers.as_deref()),
            subscription_id: s.subscription_id.clone(),
            ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
        };
        if !dup_groups.contains_key(&winner) {
            winner_order.push(winner.clone());
        }
        dup_groups.entry(winner).or_default().push(row);
    }

    let mut insertions: Vec<(usize, Vec<SubnetPrintRow>)> = winner_order
        .into_iter()
        .map(|winner_vnet| {
            let pos = output_rows
                .iter()
                .rposition(|r| r.vnet_name == winner_vnet)
                .map(|i| i + 1)
                .unwrap_or(output_rows.len());
            let rows = dup_groups.remove(&winner_vnet).unwrap_or_default();
            (pos, rows)
        })
        .collect();

    insertions.sort_by_key(|b| Reverse(b.0));
    for (pos, rows) in insertions {
        let tail = output_rows.split_off(pos);
        output_rows.extend(rows);
        output_rows.extend(tail);
    }

    // ── 4. Insert vWAN hub rows at sorted IP position ───────────────────────
    let mut hub_rows: Vec<(u32, SubnetPrintRow)> = Vec::new();
    for hub in vwan {
        if hub.hub_address_prefix.is_empty() {
            continue;
        }
        let cidr = &hub.hub_address_prefix;
        let (start_u32, _prefix_len, broadcast, az_hosts) = match parse_cidr(cidr) {
            Some(v) => v,
            None => {
                log::warn!(
                    "vWAN hub '{}' has unparseable CIDR '{cidr}' — skipped",
                    hub.hub_name
                );
                continue;
            }
        };
        let row = SubnetPrintRow {
            j: 0,
            gap: "VWAN_HUB".to_string(),
            subnet_cidr: cidr.clone(),
            broadcast,
            az_hosts,
            subnet_name: format!("vWAN Hub:{}", hub.hub_name),
            subscription_name: hub.subscription_name.clone(),
            vnet_cidr: cidr.clone(),
            vnet_name: if hub.virtual_wan_name.is_empty() {
                hub.hub_name.clone()
            } else {
                hub.virtual_wan_name.clone()
            },
            location: hub.location.clone(),
            nsg: "None".to_string(),
            dns: "None".to_string(),
            subscription_id: hub.subscription_id.clone(),
            ip_configurations_count: 0,
        };
        hub_rows.push((start_u32, row));
    }
    hub_rows.sort_by_key(|(ip, _)| *ip);

    for (hub_ip, hub_row) in hub_rows.into_iter().rev() {
        let pos = output_rows
            .iter()
            .rposition(|r| cidr_start_u32(&r.subnet_cidr).is_some_and(|ip| ip <= hub_ip))
            .map(|i| i + 1)
            .unwrap_or(0);
        output_rows.insert(pos, hub_row);
    }

    output_rows
}

/// Write subnet data as CSV to a file.
///
/// # Arguments
/// * `data`          - The active (winner) subnet data
/// * `excluded`      - Subnets excluded due to VNet overlap conflicts
/// * `gap_cidr_mask` - The default CIDR mask for gap subnets
/// * `vwan`          - vWAN hub rows; their address prefixes are injected as
///   `VWAN_HUB` rows so reserved hub IP space is visible
/// * `output_dir`    - Directory in which to write `net_<date>_subnets.csv`
///   and `net_<date>_duplicates.md`. Pass `Path::new(".")` for the current
///   directory.
///
/// # Returns
/// The path to the generated CSV file
pub fn subnet_print(
    data: &Data,
    excluded: &[ExcludedSubnet],
    gap_cidr_mask: u8,
    vwan: &[VWanRow],
    output_dir: &Path,
) -> Result<String, Box<dyn Error>> {
    log::info!(
        "#Start subnet_print() add gap subnets with mask /{}",
        gap_cidr_mask
    );
    log::info!("# Got subnet count = {} == {}", data.count, data.data.len());

    // Generate filename with current date
    let date_str = Local::now().format("%Y-%m-%d").to_string();
    let filename = output_dir
        .join(format!("net_{date_str}_subnets.csv"))
        .to_string_lossy()
        .into_owned();

    // Open file for writing
    let file = File::create(&filename)?;
    let mut writer = BufWriter::new(file);

    // Write CSV header
    writeln!(
        writer,
        r#" "cnt", "gap"  , "subnet_cidr"    ,"vms"        ,  "broadcast"      , "subnet_name"          ,  "subscription_name",     "vnet_cidr"        ,      "vnet_name","location","nsg","dns","subscription_id""#
    )?;

    let output_rows = build_rows(&data.data, excluded, gap_cidr_mask, vwan);

    // Write the subnets as CSV
    for row in &output_rows {
        write_csv_row(&mut writer, row)?;
    }

    writer.flush()?;

    log::info!("Wrote {} rows to '{}'", output_rows.len(), filename);

    // Also write the duplicates markdown report alongside the CSV
    let md_filename = output_dir
        .join(format!("net_{date_str}_duplicates.md"))
        .to_string_lossy()
        .into_owned();
    super::dup_report::write_duplicates_md(data, excluded, &md_filename)?;

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

/// Parse `"a.b.c.d/n"` into `(start_u32, prefix_len, broadcast_str, az_hosts)`.
fn parse_cidr(cidr: &str) -> Option<(u32, u8, String, usize)> {
    let (addr_str, len_str) = cidr.split_once('/')?;
    let addr: Ipv4Addr = addr_str.parse().ok()?;
    let prefix_len: u8 = len_str.parse().ok()?;
    if prefix_len > 32 {
        return None;
    }
    let start = u32::from(addr);
    let mask = if prefix_len == 0 {
        0u32
    } else {
        !0u32 << (32 - prefix_len)
    };
    let broadcast_u32 = (start & mask) | !mask;
    let broadcast_addr = Ipv4Addr::from(broadcast_u32);
    // Azure reserves 5 addresses per subnet (network, gateway, two DNS, broadcast).
    let total_ips = 1u64 << (32 - prefix_len);
    let az_hosts = total_ips.saturating_sub(5) as usize;
    Some((
        start & mask,
        prefix_len,
        broadcast_addr.to_string(),
        az_hosts,
    ))
}

/// Return the start IP of a CIDR string as a `u32`, or `None` if unparseable.
fn cidr_start_u32(cidr: &str) -> Option<u32> {
    let addr_str = cidr.split('/').next()?;
    let addr: Ipv4Addr = addr_str.parse().ok()?;
    Some(u32::from(addr))
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

        // Test first subnet via GapFinder (verifies push returns the subnet row)
        use crate::processing::GapFinder;
        let mut gf = GapFinder::new(28);
        let rows = gf.push(&result.data[0], 1);

        assert_eq!(result.data[0].subnet_name, "jenkinsarm-snet");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].subnet_cidr, "10.0.0.0/24");
    }

    #[test]
    fn excluded_subnet_skipped_in_gap_finder_and_appears_as_dup_in_csv() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};
        use crate::processing::ExcludedSubnet;

        fn make_subnet(
            vnet_name: &str,
            sub_name: &str,
            vnet_cidr: &str,
            subnet_cidr: &str,
        ) -> Subnet {
            let mut s: Subnet = Default::default();
            s.vnet_name = vnet_name.to_string();
            s.subscription_name = sub_name.to_string();
            s.subscription_id = "sub-id".to_string();
            s.vnet_cidr = Ipv4::new(vnet_cidr).unwrap();
            s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
            s.subnet_name = "my-subnet".to_string();
            s
        }

        // active: only the winner subnet; excluded: loser passed separately
        let data = Data {
            count: 1,
            skip_token: None,
            total_records: None,
            data: vec![make_subnet(
                "winner-vnet",
                "Coretex Production",
                "10.11.0.0/16",
                "10.11.4.0/22",
            )],
        };
        let excluded = vec![ExcludedSubnet {
            subnet: make_subnet("loser-vnet", "Sandbox", "10.11.0.0/16", "10.11.4.0/22"),
            winner_vnet_name: "winner-vnet".to_string(),
        }];

        let path = subnet_print(&data, &excluded, 28, &[], std::path::Path::new("."))
            .expect("subnet_print must not panic");
        let contents = std::fs::read_to_string(&path).expect("can read CSV");
        let _ = std::fs::remove_file(&path);

        assert!(
            contents.contains("DUP_EXCL_VNET"),
            "CSV must contain DUP_EXCL_VNET row"
        );
        assert!(
            contents.contains("winner-vnet"),
            "DUP row must reference the winning VNet"
        );
    }

    #[test]
    fn dup_rows_appear_directly_after_winner_vnet_not_at_end() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};
        use crate::processing::ExcludedSubnet;

        fn make_subnet(
            vnet_name: &str,
            sub_name: &str,
            vnet_cidr: &str,
            subnet_cidr: &str,
        ) -> Subnet {
            let mut s: Subnet = Default::default();
            s.vnet_name = vnet_name.to_string();
            s.subscription_name = sub_name.to_string();
            s.subscription_id = "sub-id".to_string();
            s.vnet_cidr = Ipv4::new(vnet_cidr).unwrap();
            s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
            s.subnet_name = "snet".to_string();
            s
        }

        // active: winner-vnet and later-vnet; excluded: loser-vnet
        let data = Data {
            count: 2,
            skip_token: None,
            total_records: None,
            data: vec![
                make_subnet("winner-vnet", "Prod", "10.0.0.0/16", "10.0.0.0/24"),
                make_subnet("later-vnet", "Prod", "10.1.0.0/16", "10.1.0.0/24"),
            ],
        };
        let excluded = vec![ExcludedSubnet {
            subnet: make_subnet("loser-vnet", "Dev", "10.0.0.0/16", "10.0.0.0/24"),
            winner_vnet_name: "winner-vnet".to_string(),
        }];

        let path = subnet_print(&data, &excluded, 28, &[], std::path::Path::new("."))
            .expect("must not panic");
        let contents = std::fs::read_to_string(&path).expect("can read");
        let _ = std::fs::remove_file(&path);

        let dup_pos = contents
            .find("DUP_EXCL_VNET")
            .expect("DUP_EXCL_VNET must exist");
        let later_pos = contents.find("later-vnet").expect("later-vnet must exist");

        assert!(
            dup_pos < later_pos,
            "DUP row must appear directly after winner-vnet rows, not at the end of the file.\nDUP at byte {dup_pos}, later-vnet at byte {later_pos}"
        );
    }

    #[test]
    fn trailing_vgap_within_vnet_cidr_is_filled() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};

        let mut s: Subnet = Default::default();
        s.vnet_name = "my-vnet".to_string();
        s.subscription_name = "my-sub".to_string();
        s.subscription_id = "sub-id".to_string();
        s.vnet_cidr = Ipv4::new("10.0.0.0/16").unwrap();
        s.subnet_cidr = Some(Ipv4::new("10.0.0.0/24").unwrap());
        s.subnet_name = "only-subnet".to_string();

        let data = Data {
            count: 1,
            skip_token: None,
            total_records: None,
            data: vec![s],
        };

        let path =
            subnet_print(&data, &[], 28, &[], std::path::Path::new(".")).expect("must not panic");
        let contents = std::fs::read_to_string(&path).expect("can read CSV");
        let _ = std::fs::remove_file(&path);

        assert!(
            contents.contains("-vgap-"),
            "CSV must contain -vgap- rows for trailing space within VNet_CIDR\n{contents}"
        );
        assert!(
            contents.contains("10.0.0.0/16_vnet"),
            "trailing vgap must reference the VNet_CIDR 10.0.0.0/16\n{contents}"
        );
    }

    #[test]
    fn subnet_print_also_produces_duplicates_md() {
        let _guard = CSV_FILE_LOCK.lock().unwrap();
        use crate::azure::Data;
        use crate::models::{Ipv4, Subnet};
        use crate::processing::ExcludedSubnet;

        fn make_subnet(vnet_name: &str, vnet_cidr: &str, subnet_cidr: &str) -> Subnet {
            let mut s: Subnet = Default::default();
            s.vnet_name = vnet_name.to_string();
            s.subscription_name = "Sub".to_string();
            s.subscription_id = "sub-id".to_string();
            s.vnet_cidr = Ipv4::new(vnet_cidr).unwrap();
            s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
            s.subnet_name = "snet".to_string();
            s
        }

        let data = Data {
            count: 1,
            skip_token: None,
            total_records: None,
            data: vec![make_subnet("winner-vnet", "10.0.0.0/16", "10.0.0.0/24")],
        };
        let excluded = vec![ExcludedSubnet {
            subnet: make_subnet("excl-vnet", "10.0.0.0/16", "10.0.0.0/24"),
            winner_vnet_name: "winner-vnet".to_string(),
        }];

        let csv_path = subnet_print(&data, &excluded, 28, &[], std::path::Path::new("."))
            .expect("must not panic");
        let md_path = csv_path.replace("_subnets.csv", "_duplicates.md");
        let _ = std::fs::remove_file(&csv_path);
        assert!(
            std::fs::metadata(&md_path).is_ok(),
            "duplicates.md must be created alongside CSV"
        );
        let _ = std::fs::remove_file(&md_path);
    }

    // ── build_rows unit tests ─────────────────────────────────────────────────

    fn make_subnet_for_build(
        vnet_name: &str,
        subscription_name: &str,
        vnet_cidr: &str,
        subnet_cidr: &str,
        subnet_name: &str,
    ) -> Subnet {
        let mut s: Subnet = Default::default();
        s.vnet_name = vnet_name.to_string();
        s.subscription_name = subscription_name.to_string();
        s.subscription_id = "sub-id".to_string();
        s.vnet_cidr = crate::models::Ipv4::new(vnet_cidr).unwrap();
        s.subnet_cidr = Some(crate::models::Ipv4::new(subnet_cidr).unwrap());
        s.subnet_name = subnet_name.to_string();
        s
    }

    #[test]
    fn build_rows_single_subnet_fills_vnet_exactly() {
        let subnet = make_subnet_for_build("my-vnet", "Prod", "10.0.0.0/24", "10.0.0.0/24", "snet");
        let rows = build_rows(&[subnet], &[], 28, &[]);
        assert_eq!(
            rows.len(),
            1,
            "expected 1 row, got {}: {rows:?}",
            rows.len()
        );
        assert_eq!(rows[0].gap, "");
        assert_eq!(rows[0].subnet_cidr, "10.0.0.0/24");
        assert_eq!(rows[0].vnet_name, "my-vnet");
    }

    #[test]
    fn build_rows_unused_space_in_vnet_becomes_vgap_rows() {
        let subnet = make_subnet_for_build("my-vnet", "Prod", "10.0.0.0/16", "10.0.0.0/24", "snet");
        let rows = build_rows(&[subnet], &[], 28, &[]);
        assert!(rows.len() > 1, "expected vgap rows");
        assert!(
            rows.iter().any(|r| r.gap == "-vgap-"),
            "expected -vgap- row"
        );
        let sr = rows.iter().find(|r| r.gap.is_empty()).expect("subnet row");
        assert_eq!(sr.subnet_cidr, "10.0.0.0/24");
    }

    #[test]
    fn build_rows_global_gap_between_vnets_is_gap_row() {
        let s1 = make_subnet_for_build("vnet-a", "Prod", "10.0.0.0/24", "10.0.0.0/24", "snet");
        let s2 = make_subnet_for_build("vnet-b", "Prod", "10.1.0.0/24", "10.1.0.0/24", "snet");
        let rows = build_rows(&[s1, s2], &[], 28, &[]);
        assert!(
            rows.iter().any(|r| r.gap == "-gap-"),
            "expected -gap- row between vnets"
        );
    }

    #[test]
    fn build_rows_excluded_subnet_becomes_dup_excl_vnet_row() {
        let winner =
            make_subnet_for_build("winner-vnet", "Prod", "10.0.0.0/16", "10.0.0.0/24", "snet");
        let loser =
            make_subnet_for_build("loser-vnet", "Dev", "10.0.0.0/16", "10.0.0.0/24", "snet");
        let excluded = vec![ExcludedSubnet {
            subnet: loser,
            winner_vnet_name: "winner-vnet".to_string(),
        }];
        let rows = build_rows(&[winner], &excluded, 28, &[]);
        assert!(
            rows.iter().any(|r| r.gap == "DUP_EXCL_VNET"),
            "expected DUP_EXCL_VNET row"
        );
    }

    #[test]
    fn build_rows_vwan_hub_appears_at_sorted_ip_position() {
        use crate::azure::VWanRow;
        let s1 = make_subnet_for_build("vnet-a", "Prod", "10.0.0.0/24", "10.0.0.0/24", "snet");
        let s2 = make_subnet_for_build("vnet-b", "Prod", "10.2.0.0/24", "10.2.0.0/24", "snet");
        let hub = VWanRow {
            hub_name: "my-hub".to_string(),
            hub_address_prefix: "10.1.0.0/24".to_string(),
            virtual_wan_name: "my-vwan".to_string(),
            subscription_name: "Prod".to_string(),
            subscription_id: "sub-id".to_string(),
            location: "eastus".to_string(),
        };
        let rows = build_rows(&[s1, s2], &[], 28, &[hub]);

        assert!(
            rows.iter().any(|r| r.gap == "VWAN_HUB"),
            "expected a VWAN_HUB row"
        );
        // hub (10.1.x) must appear between vnet-a (10.0.x) and vnet-b (10.2.x)
        let hub_pos = rows.iter().position(|r| r.gap == "VWAN_HUB").unwrap();
        let a_pos = rows.iter().position(|r| r.vnet_name == "vnet-a").unwrap();
        let b_pos = rows.iter().position(|r| r.vnet_name == "vnet-b").unwrap();
        assert!(
            a_pos < hub_pos && hub_pos < b_pos,
            "hub must be between vnet-a and vnet-b; positions: a={a_pos}, hub={hub_pos}, b={b_pos}"
        );
    }
}
