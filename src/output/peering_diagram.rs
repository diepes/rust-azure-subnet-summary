//! Mermaid peering diagram generator.
//!
//! Writes a `subnets-YYYY-MM-DD-peering.md` file containing a Mermaid `graph TD`
//! showing VNet peering topology grouped into Subscription Islands.
//!
//! Uses the ELK renderer (`%%{init}%%` directive) for better auto-layout of
//! dense graphs.

use super::peering_topology::{build_topology, node_id};
use crate::azure::{Data, PeeringEdge};
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};

// ─── public API ─────────────────────────────────────────────────────────────

/// Write a Mermaid peering diagram to `filename`.
///
/// * `edges`   – directed peering edges from Azure Resource Graph
/// * `subnets` – raw subnet data (used to find CIDR, subscription names, GatewaySubnets)
/// * `filename` – output path for the `.md` file
pub fn write_peering_diagram(
    edges: &[PeeringEdge],
    subnets: &Data,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let topo = build_topology(edges, subnets);

    let file = File::create(filename)?;
    let mut w = BufWriter::new(file);
    let date = chrono::Local::now().format("%Y-%m-%d");

    writeln!(w, "# Azure VNet Peering Diagram")?;
    writeln!(w)?;
    writeln!(w, "Generated: {date}")?;
    writeln!(w)?;
    writeln!(w, "```mermaid")?;
    writeln!(
        w,
        "%%{{init: {{'flowchart': {{'defaultRenderer': 'elk'}}}}}}%%"
    )?;
    writeln!(w, "graph TD")?;

    let mut link_index: usize = 0;
    let mut broken_link_indices: Vec<usize> = Vec::new();

    for (island_num, vnets) in topo.islands.iter().enumerate() {
        let all_missing = vnets
            .iter()
            .all(|v| topo.vnet_meta.get(v).map(|m| m.missing).unwrap_or(false));

        let island_label = if all_missing {
            let sub = vnets
                .iter()
                .find_map(|v| topo.vnet_meta.get(v))
                .map(|m| m.subscription_name.as_str())
                .unwrap_or("unknown");
            if sub.is_empty() {
                "MISSING".to_string()
            } else {
                format!("MISSING - SUB:{sub}")
            }
        } else {
            format!("Island {}", island_num + 1)
        };
        writeln!(w, "    subgraph \"{island_label}\"")?;

        for vnet in vnets {
            let meta = topo.vnet_meta.get(vnet);
            let sub = meta.map(|m| m.subscription_name.as_str()).unwrap_or("?");
            let cidr = meta.map(|m| m.vnet_cidr.join(", ")).unwrap_or_default();
            let nid = node_id(vnet);
            let is_missing = meta.map(|m| m.missing).unwrap_or(false);
            if is_missing {
                let sub_display = if sub.is_empty() { "unknown" } else { sub };
                writeln!(
                    w,
                    "        {nid}[\"⚠ MISSING\\nSUB:{sub_display}\\n{vnet}\"]"
                )?;
            } else {
                writeln!(w, "        {nid}[\"{sub}/{vnet}\\n{cidr}\"]")?;
            }

            if meta.map(|m| m.has_gateway).unwrap_or(false) {
                let ext = format!("{nid}_ext");
                writeln!(w, "        {ext}((\"🌐 External / On-Premises\"))")?;
                writeln!(w, "        {nid} --- {ext}")?;
                link_index += 1;
            }
        }

        // Bidir edges within this island
        let mut emitted: HashSet<(String, String)> = HashSet::new();
        for (a, b) in &topo.bidir_pairs {
            let ia = topo.island_id.get(a).copied().unwrap_or(usize::MAX);
            let ib = topo.island_id.get(b).copied().unwrap_or(usize::MAX);
            if ia != island_num || ib != island_num {
                continue;
            }
            let pair = if a <= b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            if !emitted.insert(pair) {
                continue;
            }
            writeln!(w, "        {} <--> {}", node_id(a), node_id(b))?;
            link_index += 1;
        }

        writeln!(w, "    end")?;
    }

    // Broken edges span island boundaries — rendered outside subgraphs
    for be in &topo.broken_edges {
        writeln!(w, "    {} --x {}", node_id(&be.from), node_id(&be.to))?;
        broken_link_indices.push(link_index);
        link_index += 1;
    }

    for idx in broken_link_indices {
        writeln!(w, "    linkStyle {idx} stroke:#ff0000,color:#ff0000")?;
    }

    writeln!(w, "```")?;
    Ok(())
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::Data;

    fn arm_id(sub: &str, vnet: &str) -> String {
        format!(
            "/subscriptions/{sub}/resourceGroups/rg/providers/Microsoft.Network/virtualNetworks/{vnet}"
        )
    }

    fn empty_data() -> Data {
        Data {
            data: vec![],
            count: 0,
            skip_token: None,
            total_records: None,
        }
    }

    #[test]
    fn elk_renderer_directive_present() {
        let edges = vec![PeeringEdge {
            vnet_name: "vnet-a".into(),
            peering_state: "Connected".into(),
            remote_vnet_id: arm_id("s2", "vnet-b"),
            ..Default::default()
        }];
        let f = "/tmp/test-mermaid-elk.md";
        write_peering_diagram(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("defaultRenderer") && c.contains("elk"),
            "ELK directive missing:\n{c}"
        );
    }

    #[test]
    fn two_connected_vnets_produce_bidir_arrow() {
        let edges = vec![
            PeeringEdge {
                vnet_name: "vnet-a".into(),
                subscription_name: "Sub A".into(),
                peering_state: "Connected".into(),
                remote_vnet_id: arm_id("s2", "vnet-b"),
                ..Default::default()
            },
            PeeringEdge {
                vnet_name: "vnet-b".into(),
                subscription_name: "Sub B".into(),
                peering_state: "Connected".into(),
                remote_vnet_id: arm_id("s1", "vnet-a"),
                ..Default::default()
            },
        ];
        let f = "/tmp/test-peering-bidir.md";
        write_peering_diagram(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("<-->"),
            "Expected <--> for fully connected peers:\n{c}"
        );
    }

    #[test]
    fn asymmetric_peering_produces_stop_arrow_in_red() {
        let edges = vec![PeeringEdge {
            vnet_name: "broken-vnet".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "spoke-vnet"),
            ..Default::default()
        }];
        let f = "/tmp/test-peering-broken.md";
        write_peering_diagram(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(c.contains("--x"), "Expected --x stop arrow:\n{c}");
        assert!(c.contains("stroke:#ff0000"), "Expected red stroke:\n{c}");
    }

    #[test]
    fn node_labels_include_subscription_slash_vnet() {
        let edges = vec![PeeringEdge {
            vnet_name: "my-vnet".into(),
            subscription_name: "My Sub".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "other-vnet"),
            ..Default::default()
        }];
        let f = "/tmp/test-peering-label.md";
        write_peering_diagram(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("My Sub/my-vnet"),
            "Node label must be Sub/VNet:\n{c}"
        );
    }

    #[test]
    fn gateway_vnet_gets_external_node() {
        use crate::models::Subnet;
        let mut s = Subnet::default();
        s.vnet_name = "hub-vnet".into();
        s.subnet_name = "GatewaySubnet".into();
        s.subscription_name = "Prod Sub".into();
        let data = Data {
            data: vec![s],
            count: 1,
            skip_token: None,
            total_records: None,
        };
        let f = "/tmp/test-peering-gateway.md";
        write_peering_diagram(&[], &data, f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("On-Premises") || c.contains("External"),
            "Gateway node missing:\n{c}"
        );
        assert!(c.contains("hub-vnet"), "hub-vnet must appear:\n{c}");
    }

    #[test]
    fn standalone_vnet_gets_own_subgraph() {
        use crate::models::Subnet;
        let mut s = Subnet::default();
        s.vnet_name = "standalone-vnet".into();
        s.subnet_name = "default".into();
        s.subscription_name = "Standalone Sub".into();
        let data = Data {
            data: vec![s],
            count: 1,
            skip_token: None,
            total_records: None,
        };
        let f = "/tmp/test-peering-standalone.md";
        write_peering_diagram(&[], &data, f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(c.contains("subgraph"), "Must have subgraph:\n{c}");
        assert!(
            c.contains("Standalone Sub/standalone-vnet"),
            "Must have correct label:\n{c}"
        );
    }
}
