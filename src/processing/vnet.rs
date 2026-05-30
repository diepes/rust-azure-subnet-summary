//! VNet aggregation and operations.
//!
//! Groups subnets into their parent VNets for reporting.

use crate::azure::Data;
use crate::models::VnetList;
use std::error::Error;

/// Build a VnetList from subnet data.
///
/// # Arguments
/// * `data` - The subnet data to aggregate
///
/// # Returns
/// * `Ok(VnetList)` - Aggregated VNet data
pub fn get_vnets(data: &Data) -> Result<VnetList<'_>, Box<dyn Error>> {
    let mut vnets = VnetList::new();

    for subnet in data.data.iter() {
        let key = (&subnet.vnet_name as &str, &subnet.subscription_name as &str);

        if let Some(vnet) = vnets.vnets.get_mut(&key) {
            vnet.add_subnet(subnet);
        } else {
            vnets.add_vnet(subnet);
        }
    }

    Ok(vnets)
}

/// Render VNet summary as a string, grouping conflict pairs.
///
/// Winners are shown in green. Excluded VNets are shown beneath their winner
/// with a `[DUP of <winner>]` reference.
pub fn format_vnets(vnets: &VnetList<'_>) -> String {
    use colored::Colorize;
    use std::collections::{HashMap, HashSet};

    // Separate active VNets from excluded VNets
    // A VNet is excluded if any of its subnets has excluded_by set
    let mut excluded_by_winner: HashMap<&str, Vec<&crate::models::Vnet<'_>>> = HashMap::new();
    let mut active_vnets: Vec<&crate::models::Vnet<'_>> = Vec::new();

    for vnet in vnets.vnets.values() {
        if let Some(excluded_by) = vnet.subnets.iter().find_map(|s| s.excluded_by.as_deref()) {
            excluded_by_winner
                .entry(excluded_by)
                .or_default()
                .push(vnet);
        } else {
            active_vnets.push(vnet);
        }
    }

    // Sort active VNets by subscription name then vnet name for stable output
    active_vnets.sort_by_key(|v| (v.subscription_name, v.vnet_name));

    let winner_names: HashSet<&str> = excluded_by_winner.keys().copied().collect();
    let mut lines = Vec::new();

    for vnet in &active_vnets {
        let cidrs = vnet
            .vnet_cidr
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let subnet_count = vnet.subnets.iter().filter(|s| s.excluded_by.is_none()).count();

        let line = format!(
            "VNET: '{}' '{}' - {} [{} subnet(s)]",
            vnet.vnet_name, vnet.subscription_name, cidrs, subnet_count
        );

        if winner_names.contains(vnet.vnet_name) {
            lines.push(line.green().to_string());
            // Print excluded duplicates below this winner
            if let Some(excluded) = excluded_by_winner.get(vnet.vnet_name) {
                let mut sorted_excluded = excluded.clone();
                sorted_excluded.sort_by_key(|v| (v.subscription_name, v.vnet_name));
                for excl in sorted_excluded {
                    let excl_cidrs = excl
                        .vnet_cidr
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let excl_count = excl.subnets.len();
                    lines.push(
                        format!(
                            "  EXCL: '{}' '{}' - {} [{} subnet(s)] [DUP of '{}']",
                            excl.vnet_name,
                            excl.subscription_name,
                            excl_cidrs,
                            excl_count,
                            vnet.vnet_name
                        )
                        .red()
                        .to_string(),
                    );
                }
            }
        } else {
            lines.push(line);
        }
    }

    lines.join("\n")
}

/// Print VNet summary to stdout.
pub fn print_vnets(
    vnets: &VnetList<'_>,
    _excluded_vnets: Option<&[crate::processing::VnetInfo]>,
) -> Result<(), Box<dyn Error>> {
    let total = vnets.vnets.len();
    let excluded_count = vnets
        .vnets
        .values()
        .filter(|v| v.subnets.iter().any(|s| s.excluded_by.is_some()))
        .count();
    log::info!(
        "VNETs: found {} VNETs ({} excluded as duplicates)",
        total,
        excluded_count
    );

    let output = format_vnets(vnets);
    println!("{output}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::Data;
    use crate::models::{Ipv4, Subnet};

    fn make_subnet(vnet_name: &str, sub_name: &str, vnet_cidr: &str, subnet_cidr: &str, excluded_by: Option<&str>) -> Subnet {
        let mut s: Subnet = Default::default();
        s.vnet_name = vnet_name.to_string();
        s.subscription_name = sub_name.to_string();
        s.subscription_id = "sub-id".to_string();
        s.vnet_cidr = vec![Ipv4::new(vnet_cidr).unwrap()];
        s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
        s.subnet_name = format!("{vnet_name}-subnet");
        s.excluded_by = excluded_by.map(|s| s.to_string());
        s
    }

    fn make_data(subnets: Vec<Subnet>) -> Data {
        Data {
            count: subnets.len() as i32,
            skip_token: None,
            total_records: None,
            data: subnets,
        }
    }

    #[test]
    fn excluded_vnet_appears_with_dup_reference_in_terminal_output() {
        let data = make_data(vec![
            make_subnet("winner-vnet", "Coretex Production", "10.1.0.0/16", "10.1.1.0/24", None),
            make_subnet("loser-vnet",  "Sandbox",            "10.1.0.0/16", "10.1.1.0/24", Some("winner-vnet")),
        ]);

        let vnets = get_vnets(&data).unwrap();
        let output = format_vnets(&vnets);

        // Strip ANSI codes for assertion
        let plain = strip_ansi(&output);
        assert!(plain.contains("winner-vnet"), "winner must appear in output");
        assert!(plain.contains("loser-vnet"), "excluded vnet must appear in output");
        assert!(plain.contains("DUP of 'winner-vnet'"), "excluded must reference winner");
        assert!(plain.contains("EXCL:"), "excluded vnet must be labeled EXCL");
    }

    #[test]
    fn non_overlapping_vnets_show_without_conflict_markers() {
        let data = make_data(vec![
            make_subnet("vnet-a", "Sub A", "10.1.0.0/16", "10.1.1.0/24", None),
            make_subnet("vnet-b", "Sub B", "10.2.0.0/16", "10.2.1.0/24", None),
        ]);

        let vnets = get_vnets(&data).unwrap();
        let output = format_vnets(&vnets);
        let plain = strip_ansi(&output);

        assert!(plain.contains("vnet-a"), "vnet-a must appear");
        assert!(plain.contains("vnet-b"), "vnet-b must appear");
        assert!(!plain.contains("EXCL:"), "no EXCL markers for non-overlapping");
        assert!(!plain.contains("DUP of"), "no DUP markers for non-overlapping");
    }

    /// Strip ANSI escape codes from a string for plain-text assertions.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape && c == 'm' {
                in_escape = false;
            } else if !in_escape {
                result.push(c);
            }
        }
        result
    }
}

