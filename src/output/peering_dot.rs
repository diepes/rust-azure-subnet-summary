//! Graphviz DOT peering diagram generator.
//!
//! Writes a `subnets-YYYY-MM-DD-peering.dot` file using the `fdp` force-directed
//! layout, which handles dense network topology graphs well.
//!
//! Render with:
//! ```sh
//! dot -Kfdp -Tsvg subnets-2026-05-31-peering.dot -o peering.svg
//! # or open in VSCode with the "Graphviz Preview" extension
//! ```

use super::peering_topology::{build_topology, node_id};
use crate::azure::{Data, PeeringEdge};
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};

// ─── public API ─────────────────────────────────────────────────────────────

/// Write a Graphviz DOT peering diagram to `filename`.
///
/// * `edges`   – directed peering edges from Azure Resource Graph
/// * `subnets` – raw subnet data (used to find CIDR, subscription names, GatewaySubnets)
/// * `filename` – output path for the `.dot` file
pub fn write_peering_dot(
    edges: &[PeeringEdge],
    subnets: &Data,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let topo = build_topology(edges, subnets);

    let file = File::create(filename)?;
    let mut w = BufWriter::new(file);
    let date = chrono::Local::now().format("%Y-%m-%d");

    writeln!(w, "// Azure VNet Peering Diagram — generated {date}")?;
    writeln!(w, "// Render: dot -Kfdp -Tsvg {filename} -o peering.svg")?;
    writeln!(w, "digraph azure_vnet_peering {{")?;
    writeln!(
        w,
        "    graph [layout=fdp overlap=false splines=true fontname=\"Helvetica\"]"
    )?;
    writeln!(
        w,
        "    node  [shape=box style=\"filled,rounded\" fillcolor=\"#ddeeff\" fontname=\"Helvetica\" margin=\"0.3,0.1\"]"
    )?;
    writeln!(w, "    edge  [fontname=\"Helvetica\" fontsize=10]")?;
    writeln!(w)?;

    // ── subgraphs (Subscription Islands) ─────────────────────────────────────
    for (island_num, vnets) in topo.islands.iter().enumerate() {
        let all_missing = vnets
            .iter()
            .all(|v| topo.vnet_meta.get(v).map(|m| m.missing).unwrap_or(false));
        let any_missing = vnets
            .iter()
            .any(|v| topo.vnet_meta.get(v).map(|m| m.missing).unwrap_or(false));

        let (cluster_fill, cluster_font_color, cluster_label) = if all_missing {
            // Derive a readable label from the first missing VNet's subscription
            let sub = vnets
                .iter()
                .find_map(|v| topo.vnet_meta.get(v))
                .map(|m| m.subscription_name.as_str())
                .unwrap_or("unknown");
            let label = if sub.is_empty() {
                "MISSING".to_string()
            } else {
                format!("MISSING - SUB:{sub}")
            };
            ("#8b0000", "white", label)
        } else if any_missing {
            let label = format!("Island {} [⚠ missing peers]", island_num + 1);
            ("#ffe0e0", "black", label)
        } else {
            let colour = ISLAND_COLOURS[island_num % ISLAND_COLOURS.len()];
            (colour, "black", format!("Island {}", island_num + 1))
        };

        writeln!(w, "    subgraph cluster_{island_num} {{")?;
        writeln!(w, "        label=\"{cluster_label}\"")?;
        writeln!(
            w,
            "        style=filled fillcolor=\"{cluster_fill}\" fontcolor=\"{cluster_font_color}\""
        )?;
        writeln!(w, "        fontname=\"Helvetica\" fontsize=12")?;

        for vnet in vnets {
            let meta = topo.vnet_meta.get(vnet);
            let sub = meta.map(|m| m.subscription_name.as_str()).unwrap_or("?");
            let cidr = meta.map(|m| m.vnet_cidr.join("\\n")).unwrap_or_default();
            let nid = node_id(vnet);
            let is_missing = meta.map(|m| m.missing).unwrap_or(false);

            let label = if is_missing {
                let sub_display = if sub.is_empty() { "unknown" } else { sub };
                if cidr.is_empty() {
                    format!("⚠ MISSING\\nSUB:{sub_display}\\n{vnet}")
                } else {
                    format!("⚠ MISSING\\nSUB:{sub_display}\\n{vnet}\\n{cidr}")
                }
            } else if cidr.is_empty() {
                format!("{sub}/{vnet}")
            } else {
                format!("{sub}/{vnet}\\n{cidr}")
            };

            let fill = if is_missing {
                " fillcolor=\"#cc3333\" fontcolor=\"white\""
            } else if meta.map(|m| m.has_gateway).unwrap_or(false) {
                " fillcolor=\"#fff3b0\""
            } else {
                ""
            };
            writeln!(w, "        {nid} [label=\"{label}\"{fill}]")?;

            if meta.map(|m| m.has_gateway).unwrap_or(false) {
                let ext = format!("{nid}_ext");
                writeln!(
                    w,
                    "        {ext} [label=\"External\\nOn-Premises\" shape=ellipse style=\"filled\" fillcolor=\"#c8e6c9\"]"
                )?;
                writeln!(w, "        {nid} -> {ext} [dir=none style=dotted]")?;
            }
        }
        writeln!(w, "    }}")?;
        writeln!(w)?;
    }

    // ── bidir edges ───────────────────────────────────────────────────────────
    if !topo.bidir_pairs.is_empty() {
        writeln!(w, "    // Fully connected (bidirectional) peerings")?;
        let mut sorted_bidir: Vec<&(String, String)> = topo.bidir_pairs.iter().collect();
        sorted_bidir.sort();
        for (a, b) in sorted_bidir {
            writeln!(w, "    {} -> {} [dir=both]", node_id(a), node_id(b))?;
        }
        writeln!(w)?;
    }

    // ── broken / asymmetric edges ─────────────────────────────────────────────
    if !topo.broken_edges.is_empty() {
        writeln!(w, "    // Broken / asymmetric peerings")?;
        for be in &topo.broken_edges {
            writeln!(
                w,
                "    {} -> {} [color=red style=dashed penwidth=2 label=\"broken\"]",
                node_id(&be.from),
                node_id(&be.to)
            )?;
        }
        writeln!(w)?;
    }

    writeln!(w, "}}")?;
    Ok(())
}

/// Pastel background colours cycled per island so clusters are visually distinct.
const ISLAND_COLOURS: &[&str] = &[
    "#f0f4ff", "#fff8f0", "#f0fff4", "#fff0f4", "#f4f0ff", "#f0ffff", "#fffff0",
];

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
    fn dot_file_starts_with_digraph() {
        let f = "/tmp/test-dot-header.dot";
        write_peering_dot(&[], &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("digraph azure_vnet_peering"),
            "Must start with digraph:\n{c}"
        );
        assert!(c.contains("layout=fdp"), "Must specify fdp layout:\n{c}");
    }

    #[test]
    fn dot_bidir_uses_dir_both() {
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
        let f = "/tmp/test-dot-bidir.dot";
        write_peering_dot(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("dir=both"),
            "Bidir edge must have dir=both:\n{c}"
        );
    }

    #[test]
    fn dot_broken_edge_is_red_dashed() {
        let edges = vec![PeeringEdge {
            vnet_name: "broken-vnet".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "spoke-vnet"),
            ..Default::default()
        }];
        let f = "/tmp/test-dot-broken.dot";
        write_peering_dot(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(c.contains("color=red"), "Broken edge must be red:\n{c}");
        assert!(
            c.contains("style=dashed"),
            "Broken edge must be dashed:\n{c}"
        );
    }

    #[test]
    fn dot_node_label_includes_sub_slash_vnet() {
        let edges = vec![PeeringEdge {
            vnet_name: "my-vnet".into(),
            subscription_name: "My Sub".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "other-vnet"),
            ..Default::default()
        }];
        let f = "/tmp/test-dot-label.dot";
        write_peering_dot(&edges, &empty_data(), f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("My Sub/my-vnet"),
            "Node label must include Sub/VNet:\n{c}"
        );
    }

    #[test]
    fn dot_gateway_vnet_has_external_node() {
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
        let f = "/tmp/test-dot-gateway.dot";
        write_peering_dot(&[], &data, f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("On-Premises") || c.contains("External"),
            "Gateway VNet must have external node:\n{c}"
        );
        assert!(c.contains("hub-vnet"), "hub-vnet must appear:\n{c}");
    }

    #[test]
    fn dot_standalone_vnet_in_cluster() {
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
        let f = "/tmp/test-dot-standalone.dot";
        write_peering_dot(&[], &data, f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("subgraph cluster_"),
            "Must have cluster subgraph:\n{c}"
        );
        assert!(
            c.contains("standalone-vnet"),
            "VNet must appear in diagram:\n{c}"
        );
    }
}
