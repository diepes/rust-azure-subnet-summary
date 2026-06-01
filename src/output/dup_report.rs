//! Markdown report for duplicate (excluded) VNets.

use crate::azure::Data;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};

/// Write a markdown summary of duplicate (excluded) VNets and their subnets.
///
/// For each "winning" VNet, lists every excluded VNet that was deduplicated
/// against it, along with all subnets in those excluded VNets.
pub fn write_duplicates_md(data: &Data, filename: &str) -> Result<(), Box<dyn Error>> {
    // Group excluded subnets: winner_vnet_name → excl_vnet_name → Vec<subnet>
    let mut groups: HashMap<String, HashMap<String, Vec<&crate::models::Subnet>>> = HashMap::new();
    let mut winner_order: Vec<String> = Vec::new();

    for s in data.data.iter().filter(|s| s.excluded_by.is_some()) {
        let winner = s.excluded_by.as_deref().unwrap_or("?").to_string();
        let entry = groups.entry(winner.clone()).or_insert_with(|| {
            winner_order.push(winner.clone());
            HashMap::new()
        });
        entry.entry(s.vnet_name.clone()).or_default().push(s);
    }

    // Find the first non-excluded subnet for a vnet to get its CIDR + subscription
    let winner_info = |vnet_name: &str| -> (String, String) {
        data.data
            .iter()
            .find(|s| s.excluded_by.is_none() && s.vnet_name == vnet_name)
            .map(|s| (s.vnet_cidr.to_string(), s.subscription_name.clone()))
            .unwrap_or_default()
    };

    let date_part = filename
        .trim_start_matches("net_")
        .trim_end_matches("_duplicates.md");

    let file = File::create(filename)?;
    let mut w = BufWriter::new(file);

    writeln!(w, "# Duplicate VNet Summary — {date_part}")?;

    if winner_order.is_empty() {
        writeln!(w, "\n_No duplicate VNets found._")?;
        return Ok(());
    }

    for winner_vnet in &winner_order {
        let (winner_cidr, winner_sub) = winner_info(winner_vnet);
        writeln!(
            w,
            "\n## Winner VNET: `{winner_vnet}` ({winner_cidr}) — {winner_sub}"
        )?;

        let excl_map = &groups[winner_vnet];
        let mut excl_names: Vec<&String> = excl_map.keys().collect();
        excl_names.sort();

        for excl_vnet in excl_names {
            let subnets = &excl_map[excl_vnet];
            let excl_cidr = subnets
                .first()
                .map(|s| s.vnet_cidr.to_string())
                .unwrap_or_default();
            let excl_sub = subnets
                .first()
                .map(|s| s.subscription_name.as_str())
                .unwrap_or("");

            writeln!(
                w,
                "\n### Duplicate VNET: `{excl_vnet}` ({excl_cidr}) — {excl_sub}"
            )?;
            writeln!(w, "| Subnet | CIDR |")?;
            writeln!(w, "|--------|------|")?;
            for s in subnets.iter() {
                let cidr = s.subnet_cidr.map(|c| c.to_string()).unwrap_or_default();
                writeln!(w, "| `{}` | {} |", s.subnet_name, cidr)?;
            }
        }
    }

    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Ipv4, Subnet};

    fn make_subnet(
        vnet_name: &str,
        sub_name: &str,
        vnet_cidr: &str,
        subnet_cidr: &str,
        subnet_name: &str,
        excluded_by: Option<&str>,
    ) -> Subnet {
        let mut s: Subnet = Default::default();
        s.vnet_name = vnet_name.to_string();
        s.subscription_name = sub_name.to_string();
        s.subscription_id = "sub-id".to_string();
        s.vnet_cidr = Ipv4::new(vnet_cidr).unwrap();
        s.subnet_cidr = Some(Ipv4::new(subnet_cidr).unwrap());
        s.subnet_name = subnet_name.to_string();
        s.excluded_by = excluded_by.map(|s| s.to_string());
        s
    }

    #[test]
    fn duplicates_md_contains_winner_and_excluded_vnet_sections() {
        let subnets = vec![
            make_subnet(
                "winner-vnet",
                "Prod Sub",
                "10.0.0.0/16",
                "10.0.0.0/24",
                "web-snet",
                None,
            ),
            make_subnet(
                "winner-vnet",
                "Prod Sub",
                "10.0.0.0/16",
                "10.0.1.0/24",
                "app-snet",
                None,
            ),
            make_subnet(
                "excl-vnet",
                "Dev Sub",
                "10.0.0.0/16",
                "10.0.0.0/24",
                "dup-web",
                Some("winner-vnet"),
            ),
            make_subnet(
                "excl-vnet",
                "Dev Sub",
                "10.0.0.0/16",
                "10.0.1.0/24",
                "dup-app",
                Some("winner-vnet"),
            ),
        ];
        let data = crate::azure::Data {
            count: subnets.len() as i32,
            skip_token: None,
            total_records: None,
            data: subnets,
        };

        let filename = "subnets-test-duplicates.md";
        write_duplicates_md(&data, filename).expect("must not fail");
        let contents = std::fs::read_to_string(filename).expect("file must exist");
        let _ = std::fs::remove_file(filename);

        assert!(contents.contains("winner-vnet"), "must mention winner VNet");
        assert!(
            contents.contains("Prod Sub"),
            "must mention winner subscription"
        );
        assert!(contents.contains("excl-vnet"), "must mention excluded VNet");
        assert!(
            contents.contains("Dev Sub"),
            "must mention excluded subscription"
        );
        assert!(contents.contains("dup-web"), "must list excluded subnets");
        assert!(contents.contains("dup-app"), "must list excluded subnets");
        assert!(
            contents.contains("10.0.0.0/24"),
            "must include subnet CIDRs"
        );
    }

    #[test]
    fn duplicates_md_no_duplicates_writes_placeholder() {
        let subnets = vec![make_subnet(
            "only-vnet",
            "Prod",
            "10.0.0.0/16",
            "10.0.0.0/24",
            "snet",
            None,
        )];
        let data = crate::azure::Data {
            count: 1,
            skip_token: None,
            total_records: None,
            data: subnets,
        };

        let filename = "subnets-test-no-dup-duplicates.md";
        write_duplicates_md(&data, filename).expect("must not fail");
        let contents = std::fs::read_to_string(filename).expect("file must exist");
        let _ = std::fs::remove_file(filename);

        assert!(contents.contains("_No duplicate VNets found._"));
    }
}
