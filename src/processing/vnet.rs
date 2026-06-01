//! VNet aggregation and operations.
//!
//! Groups subnets into their parent VNets for reporting.

use crate::azure::Data;
use crate::models::VnetList;
use crate::processing::ExcludedSubnet;
use std::collections::HashMap;
use std::error::Error;

// winner_vnet_name → excl_vnet_name → (subscription_name, CIDRs, count)
type ExcludedByWinner<'a> = HashMap<&'a str, HashMap<String, (String, Vec<String>, usize)>>;

/// Build a VnetList from subnet data.
///
/// # Arguments
/// * `data` - The active subnet data to aggregate
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
pub fn format_vnets(vnets: &VnetList<'_>, excluded: &[ExcludedSubnet]) -> String {
    use colored::Colorize;
    use std::collections::HashSet;

    // Build: winner_vnet_name → excl_vnet_name → (subscription_name, CIDRs, count)
    let mut excluded_by_winner: ExcludedByWinner<'_> = HashMap::new();

    for e in excluded {
        let winner = e.winner_vnet_name.as_str();
        let inner = excluded_by_winner.entry(winner).or_default();
        let entry = inner
            .entry(e.subnet.vnet_name.clone())
            .or_insert_with(|| (e.subnet.subscription_name.clone(), Vec::new(), 0));
        let cidr_str = e.subnet.vnet_cidr.to_string();
        if !entry.1.contains(&cidr_str) {
            entry.1.push(cidr_str);
        }
        entry.2 += 1;
    }

    let winner_names: HashSet<&str> = excluded_by_winner.keys().copied().collect();

    // All vnets in the list are active (no excluded subnets in data anymore)
    let mut active_vnets: Vec<&crate::models::Vnet<'_>> = vnets.vnets.values().collect();
    active_vnets.sort_by_key(|v| (v.subscription_name, v.vnet_name));

    let mut lines = Vec::new();

    for vnet in &active_vnets {
        let cidrs = vnet
            .vnet_cidr
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let subnet_count = vnet.subnets.len();

        let line = format!(
            "VNET: '{}' '{}' - {} [{} subnet(s)]",
            vnet.vnet_name, vnet.subscription_name, cidrs, subnet_count
        );

        if winner_names.contains(vnet.vnet_name) {
            lines.push(line.green().to_string());
            if let Some(excl_map) = excluded_by_winner.get(vnet.vnet_name) {
                let mut excl_names: Vec<&String> = excl_map.keys().collect();
                excl_names.sort();
                for excl_name in excl_names {
                    let (sub_name, excl_cidrs, count) = &excl_map[excl_name];
                    lines.push(
                        format!(
                            "  EXCL: '{}' '{}' - {} [{} subnet(s)] [DUP of '{}']",
                            excl_name,
                            sub_name,
                            excl_cidrs.join(", "),
                            count,
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
    excluded: &[ExcludedSubnet],
) -> Result<(), Box<dyn Error>> {
    let total = vnets.vnets.len();
    let excluded_vnet_count = {
        use std::collections::HashSet;
        excluded
            .iter()
            .map(|e| e.subnet.vnet_name.as_str())
            .collect::<HashSet<_>>()
            .len()
    };
    log::info!(
        "VNETs: found {} VNETs ({} excluded as duplicates)",
        total,
        excluded_vnet_count,
    );

    let output = format_vnets(vnets, excluded);
    println!("{output}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::Data;
    use crate::models::{Ipv4, Subnet};

    fn make_subnet(vnet_name: &str, sub_name: &str, vnet_cidr: &str, subnet_cidr: &str) -> Subnet {
        let mut s: Subnet = Default::default();
        s.vnet_name = vnet_name.to_string();
        s.subscription_name = sub_name.to_string();
        s.subscription_id = "sub-id".to_string();
        s.vnet_cidr = Ipv4::new(vnet_cidr).unwrap();
        s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
        s.subnet_name = format!("{vnet_name}-subnet");
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
        let active = make_data(vec![make_subnet(
            "winner-vnet",
            "Coretex Production",
            "10.1.0.0/16",
            "10.1.1.0/24",
        )]);
        let excluded = vec![ExcludedSubnet {
            subnet: make_subnet("loser-vnet", "Sandbox", "10.1.0.0/16", "10.1.1.0/24"),
            winner_vnet_name: "winner-vnet".to_string(),
        }];

        let vnets = get_vnets(&active).unwrap();
        let output = format_vnets(&vnets, &excluded);

        let plain = strip_ansi(&output);
        assert!(
            plain.contains("winner-vnet"),
            "winner must appear in output"
        );
        assert!(
            plain.contains("loser-vnet"),
            "excluded vnet must appear in output"
        );
        assert!(
            plain.contains("DUP of 'winner-vnet'"),
            "excluded must reference winner"
        );
        assert!(
            plain.contains("EXCL:"),
            "excluded vnet must be labeled EXCL"
        );
    }

    #[test]
    fn non_overlapping_vnets_show_without_conflict_markers() {
        let data = make_data(vec![
            make_subnet("vnet-a", "Sub A", "10.1.0.0/16", "10.1.1.0/24"),
            make_subnet("vnet-b", "Sub B", "10.2.0.0/16", "10.2.1.0/24"),
        ]);

        let vnets = get_vnets(&data).unwrap();
        let output = format_vnets(&vnets, &[]);
        let plain = strip_ansi(&output);

        assert!(plain.contains("vnet-a"), "vnet-a must appear");
        assert!(plain.contains("vnet-b"), "vnet-b must appear");
        assert!(
            !plain.contains("EXCL:"),
            "no EXCL markers for non-overlapping"
        );
        assert!(
            !plain.contains("DUP of"),
            "no DUP markers for non-overlapping"
        );
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
