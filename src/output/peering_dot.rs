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
use crate::azure::{Data, LocalGatewayRow, PeeringEdge, VWanRow};
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ─── public API ─────────────────────────────────────────────────────────────

/// Write a Graphviz DOT peering diagram to `filename`.
///
/// * `edges`          – directed peering edges from Azure Resource Graph
/// * `subnets`        – raw subnet data (used to find CIDR, subscription names, GatewaySubnets)
/// * `local_gateways` – Local Network Gateway rows (on-premises CIDRs per gateway VNet)
/// * `filename`       – output path for the `.dot` file
pub fn write_peering_dot(
    edges: &[PeeringEdge],
    subnets: &Data,
    local_gateways: &[LocalGatewayRow],
    vwan: &[VWanRow],
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let topo = build_topology(edges, subnets, local_gateways, vwan);

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

    // ── de-duplicate on-premises nodes ────────────────────────────────────────
    // Build a lookup: LNG name → LocalGatewayRow (first-seen wins per LNG name).
    let mut lng_lookup: std::collections::HashMap<&str, &LocalGatewayRow> =
        std::collections::HashMap::new();
    for row in local_gateways {
        lng_lookup.entry(row.local_gw_name.as_str()).or_insert(row);
    }

    // Key: sorted LNG names ("|"-joined), or "vnet__<name>" when no LNG is known.
    // Value: (lng_names, merged_cidrs, gateway_vnet_names)
    type ExtEntry = (Vec<String>, Vec<String>, Vec<String>);
    let mut ext_map: std::collections::BTreeMap<String, ExtEntry> =
        std::collections::BTreeMap::new();
    for (vnet_name, meta) in &topo.vnet_meta {
        if !meta.has_gateway {
            continue;
        }
        let mut sorted_names = meta.on_prem_names.clone();
        sorted_names.sort();
        // Skip gateway VNets that have no LNG connections — those are likely
        // ExpressRoute or unused gateways. The GatewaySubnet line in the VNet
        // node label already captures the subnet information.
        if sorted_names.is_empty() {
            continue;
        }
        let key = sorted_names.join("|");
        let entry = ext_map
            .entry(key)
            .or_insert_with(|| (sorted_names.clone(), Vec::new(), Vec::new()));
        for cidr in &meta.on_prem_cidrs {
            if !entry.1.contains(cidr) {
                entry.1.push(cidr.clone());
            }
        }
        if !entry.2.contains(vnet_name) {
            entry.2.push(vnet_name.clone());
        }
    }

    // Reverse map: gateway VNet name → ext node id (used when emitting edges).
    let mut vnet_to_ext: std::collections::HashMap<&str, String> =
        std::collections::HashMap::new();
    for (key, (_, _, vnets)) in &ext_map {
        let ext_id = format!("ext_{}", sanitize_id(key));
        for v in vnets {
            vnet_to_ext.insert(v.as_str(), ext_id.clone());
        }
    }

    // ── top-level on-premises nodes (outside all Islands) ────────────────────
    if !ext_map.is_empty() {
        writeln!(w, "    // On-Premises nodes (outside all Islands)")?;
        for (key, (lng_names, cidrs, _gateway_vnets)) in &ext_map {
            let ext_id = format!("ext_{}", sanitize_id(key));
            // ext_map only contains entries with non-empty lng_names (fallback was removed).
            let mut sections: Vec<String> = Vec::new();
            for lng_name in lng_names {
                let mut lines: Vec<String> = vec![format!("🌐 LNG:{lng_name}")];
                if let Some(row) = lng_lookup.get(lng_name.as_str()) {
                    let pub_ip = if !row.gateway_ip.is_empty() {
                        row.gateway_ip.clone()
                    } else if !row.gateway_ips.is_empty() {
                        row.gateway_ips.join(",")
                    } else {
                        String::new()
                    };
                    if !pub_ip.is_empty() {
                        lines.push(format!("PubIP:{pub_ip}"));
                    }
                    if !row.bgp_asn.is_empty() {
                        let bgp_line = if row.bgp_peer_ip.is_empty() {
                            format!("BGP ASN:{}", row.bgp_asn)
                        } else {
                            format!("BGP ASN:{} Peer:{}", row.bgp_asn, row.bgp_peer_ip)
                        };
                        lines.push(bgp_line);
                    }
                    for cidr in &row.address_prefixes {
                        if !cidr.is_empty() {
                            lines.push(cidr.clone());
                        }
                    }
                } else {
                    // No row data — use merged CIDRs from ext_map.
                    for cidr in cidrs {
                            if !cidr.is_empty() {
                                lines.push(cidr.clone());
                            }
                        }
                }
                sections.push(lines.join("\\n"));
            }
            let label = sections.join("\\n---\\n");
            writeln!(
                w,
                "    {ext_id} [label=\"{label}\" shape=ellipse style=\"filled\" fillcolor=\"#b3d9f7\"]"
            )?;
        }
        writeln!(w)?;
    }

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

        // ── subscription sub-clusters (inner boxes) ───────────────────────────
        // Group VNets by subscription name, preserving alphabetical order.
        let mut by_sub: std::collections::BTreeMap<&str, Vec<&str>> =
            std::collections::BTreeMap::new();
        for vnet in vnets {
            let sub = topo
                .vnet_meta
                .get(vnet)
                .map(|m| m.subscription_name.as_str())
                .unwrap_or("");
            by_sub.entry(sub).or_default().push(vnet.as_str());
        }

        for (sub_idx, (sub, sub_vnets)) in by_sub.iter().enumerate() {
            let sub_label = if sub.is_empty() { "unknown" } else { sub };
            writeln!(
                w,
                "        subgraph cluster_sub_{island_num}_{sub_idx} {{"
            )?;
            writeln!(w, "            label=\"{sub_label}\"")?;
            writeln!(
                w,
                "            style=\"filled,dashed\" fillcolor=\"white\" color=\"#666666\""
            )?;
            writeln!(w, "            fontname=\"Helvetica\" fontsize=11")?;

            for vnet in sub_vnets {
                let meta = topo.vnet_meta.get(*vnet);
                let nid = node_id(vnet);
                let is_missing = meta.map(|m| m.missing).unwrap_or(false);

                // Missing VNets use a plain string label; present VNets use an
                // HTML label so the VNG line can be rendered in bold red.
                let fill = if is_missing {
                    " fillcolor=\"#cc3333\" fontcolor=\"white\""
                } else if meta.map(|m| m.has_gateway).unwrap_or(false) {
                    " fillcolor=\"#fff3b0\""
                } else {
                    ""
                };

                if is_missing {
                    let sub_name = meta.map(|m| m.subscription_name.as_str()).unwrap_or("?");
                    let sub_display = if sub_name.is_empty() { "unknown" } else { sub_name };
                    let cidr_str = meta.map(|m| m.vnet_cidr.join("\\n")).unwrap_or_default();
                    let label = if cidr_str.is_empty() {
                        format!("⚠ MISSING\\nSUB:{sub_display}\\n{vnet}")
                    } else {
                        format!("⚠ MISSING\\nSUB:{sub_display}\\n{vnet}\\n{cidr_str}")
                    };
                    writeln!(w, "            {nid} [label=\"{label}\"{fill}]")?;
                } else {
                    let cidr_str = meta
                        .map(|m| m.vnet_cidr.join(", "))
                        .unwrap_or_default();
                    let mut parts: Vec<String> = vec![html_escape(&format!("VNET:{vnet}"))];
                    if !cidr_str.is_empty() {
                        parts.push(html_escape(&format!("CIDR:{cidr_str}")));
                    }
                    // Subnets sorted by IP address.
                    let mut vnet_subnets: Vec<&crate::models::Subnet> = subnets
                        .data
                        .iter()
                        .filter(|s| s.vnet_name == *vnet && s.excluded_by.is_none())
                        .collect();
                    vnet_subnets.sort_by_key(|s| {
                        s.subnet_cidr
                            .map(|c| u32::from_be_bytes(c.addr.octets()))
                            .unwrap_or(u32::MAX)
                    });
                    let vng_name = meta.and_then(|m| m.vng_name.as_deref()).unwrap_or("");
                    let vng_bgp_asn = meta.and_then(|m| m.vng_bgp_asn.as_deref()).unwrap_or("");
                    for s in &vnet_subnets {
                        let subnet_line = match s.subnet_cidr {
                            Some(c) => format!("Subnet:{} CIDR:{c}", s.subnet_name),
                            None => format!("Subnet:{}", s.subnet_name),
                        };
                        parts.push(html_escape(&subnet_line));
                        if s.subnet_name.eq_ignore_ascii_case("GatewaySubnet")
                            && !vng_name.is_empty()
                        {
                            let vng_line = if !vng_bgp_asn.is_empty() {
                                format!("  └ VNG:{vng_name} BGP:ASN:{vng_bgp_asn}")
                            } else {
                                format!("  └ VNG:{vng_name}")
                            };
                            parts.push(format!(
                                "<B><FONT COLOR=\"darkred\">{}</FONT></B>",
                                html_escape(&vng_line)
                            ));
                        }
                    }
                    let br = "<BR ALIGN=\"LEFT\"/>";
                    let inner = parts.join(br);
                    writeln!(w, "            {nid} [label=<{inner}{br}>{fill}]")?;
                }
            }
            writeln!(w, "        }}")?;
        }

        writeln!(w, "    }}")?;
        writeln!(w)?;
    }

    // ── vWAN Hub nodes and spoke edges ────────────────────────────────────────
    if !topo.vwan_hubs.is_empty() {
        writeln!(w, "    // vWAN Hub nodes")?;
        for hub in &topo.vwan_hubs {
            let hub_id = format!("vwan_{}", sanitize_id(&hub.hub_name));
            let mut label_parts = vec![format!("vWAN Hub:{}", hub.hub_name)];
            if !hub.hub_address_prefix.is_empty() {
                label_parts.push(format!("CIDR:{}", hub.hub_address_prefix));
            }
            if !hub.virtual_wan_name.is_empty() {
                label_parts.push(format!("vWAN:{}", hub.virtual_wan_name));
            }
            let label = label_parts.join("\\n");
            writeln!(
                w,
                "    {hub_id} [label=\"{label}\" shape=diamond style=\"filled\" fillcolor=\"#e8d5f5\" color=\"#6a0dad\" penwidth=2]"
            )?;
        }
        writeln!(w)?;
        writeln!(w, "    // Spoke VNet → vWAN Hub connections")?;
        for hub in &topo.vwan_hubs {
            let hub_id = format!("vwan_{}", sanitize_id(&hub.hub_name));
            let mut spokes: Vec<&str> = hub.spoke_vnets.iter().map(|s| s.as_str()).collect();
            spokes.sort();
            for spoke in spokes {
                // Only draw the edge if the spoke VNet actually appears in the diagram.
                if topo.vnet_meta.contains_key(spoke) {
                    writeln!(
                        w,
                        "    {} -> {hub_id} [dir=none color=\"#6a0dad\" penwidth=1.5]",
                        node_id(spoke)
                    )?;
                }
            }
        }
        writeln!(w)?;
    }

    // ── gateway VNet → on-premises edges ─────────────────────────────────────
    if !vnet_to_ext.is_empty() {
        writeln!(w, "    // Gateway VNet → On-Premises connections")?;
        let mut sorted_pairs: Vec<(&str, &String)> =
            vnet_to_ext.iter().map(|(&k, v)| (k, v)).collect();
        sorted_pairs.sort_by_key(|(k, _)| *k);
        for (vnet, ext_id) in sorted_pairs {
            writeln!(w, "    {} -> {ext_id} [dir=none color=\"#1a5fa8\" penwidth=2]", node_id(vnet))?;
        }
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

/// Sanitise an arbitrary string for use as a Graphviz node/cluster identifier.
fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
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
    fn dot_vwan_hub_node_rendered() {
        use crate::azure::VWanRow;
        use crate::models::Subnet;
        let mut s = Subnet::default();
        s.vnet_name = "spoke-vnet".into();
        s.subnet_name = "default".into();
        s.subscription_name = "Prod Sub".into();
        let data = Data {
            data: vec![s],
            count: 1,
            skip_token: None,
            total_records: None,
        };
        let vwan_row = VWanRow {
            hub_name: "prod-hub".into(),
            hub_address_prefix: "10.100.0.0/23".into(),
            virtual_wan_name: "prod-vwan".into(),
            spoke_vnet_name: "spoke-vnet".into(),
            remote_vnet_id: arm_id("x", "spoke-vnet"),
            ..Default::default()
        };
        let f = "/tmp/test-dot-vwan-hub.dot";
        write_peering_dot(&[], &data, &[], &[vwan_row], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("vWAN Hub:prod-hub"),
            "vWAN hub node must be rendered:\n{c}"
        );
        assert!(
            c.contains("shape=diamond"),
            "vWAN hub must use diamond shape:\n{c}"
        );
        assert!(
            c.contains("#e8d5f5"),
            "vWAN hub must have light purple fill:\n{c}"
        );
    }

    #[test]
    fn dot_vwan_spoke_to_hub_edge_rendered() {
        use crate::azure::VWanRow;
        use crate::models::Subnet;
        let mut s = Subnet::default();
        s.vnet_name = "spoke-vnet".into();
        s.subnet_name = "default".into();
        s.subscription_name = "Prod Sub".into();
        let data = Data {
            data: vec![s],
            count: 1,
            skip_token: None,
            total_records: None,
        };
        let vwan_row = VWanRow {
            hub_name: "prod-hub".into(),
            hub_address_prefix: "10.100.0.0/23".into(),
            virtual_wan_name: "prod-vwan".into(),
            spoke_vnet_name: "spoke-vnet".into(),
            remote_vnet_id: arm_id("x", "spoke-vnet"),
            ..Default::default()
        };
        let f = "/tmp/test-dot-vwan-edge.dot";
        write_peering_dot(&[], &data, &[], &[vwan_row], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("vwan_prod_hub"),
            "Hub node ID must appear in diagram:\n{c}"
        );
        assert!(
            c.contains("#6a0dad"),
            "Spoke→hub edge must use purple colour:\n{c}"
        );
    }

    #[test]
    fn dot_hv_prefix_peering_not_shown_as_broken() {
        use crate::azure::PeeringEdge;
        // Spoke has a one-way peering to an HV_ fabric VNet (vWAN). Should NOT
        // appear as a broken edge — it will be rendered as a vWAN hub edge instead.
        let edges = vec![PeeringEdge {
            vnet_name: "spoke-vnet".into(),
            remote_vnet_id: arm_id("s1", "HV_prod-hub_abc123"),
            peering_state: "Connected".into(),
            subscription_name: "Prod Sub".into(),
            ..Default::default()
        }];
        let f = "/tmp/test-dot-hv-hidden.dot";
        write_peering_dot(&edges, &empty_data(), &[], &[], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            !c.contains("broken"),
            "HV_ peering must NOT appear as broken edge:\n{c}"
        );
        assert!(
            !c.contains("HV_"),
            "HV_ internal VNet must NOT appear as a node:\n{c}"
        );
    }

    #[test]
    fn dot_file_starts_with_digraph() {
        let f = "/tmp/test-dot-header.dot";
        write_peering_dot(&[], &empty_data(), &[], &[], f).unwrap();
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
        write_peering_dot(&edges, &empty_data(), &[], &[], f).unwrap();
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
        write_peering_dot(&edges, &empty_data(), &[], &[], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(c.contains("color=red"), "Broken edge must be red:\n{c}");
        assert!(
            c.contains("style=dashed"),
            "Broken edge must be dashed:\n{c}"
        );
    }

    #[test]
    fn dot_node_label_includes_vnet_prefix() {
        let edges = vec![PeeringEdge {
            vnet_name: "my-vnet".into(),
            subscription_name: "My Sub".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "other-vnet"),
            ..Default::default()
        }];
        let f = "/tmp/test-dot-label.dot";
        write_peering_dot(&edges, &empty_data(), &[], &[], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("VNET:my-vnet"),
            "Node label must include VNET: prefix:\n{c}"
        );
    }

    #[test]
    fn dot_gateway_vnet_has_external_node() {
        use crate::azure::LocalGatewayRow;
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
        let lng = LocalGatewayRow {
            vnet_name: "hub-vnet".into(),
            vng_name: "hub-vpngw".into(),
            local_gw_name: "on-prem-lng".into(),
            address_prefixes: vec!["10.0.0.0/8".into()],
            gateway_ip: "1.2.3.4".into(),
            ..Default::default()
        };
        let f = "/tmp/test-dot-gateway.dot";
        write_peering_dot(&[], &data, &[lng], &[], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            c.contains("shape=ellipse"),
            "Gateway VNet with LNG must have external ellipse node:\n{c}"
        );
        assert!(
            c.contains("LNG:on-prem-lng"),
            "Ext node must show LNG: prefix:\n{c}"
        );
        assert!(c.contains("hub-vnet"), "hub-vnet must appear:\n{c}");
    }

    #[test]
    fn dot_gateway_vnet_no_lng_has_no_external_node() {
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
        let f = "/tmp/test-dot-gateway-no-lng.dot";
        write_peering_dot(&[], &data, &[], &[], f).unwrap();
        let c = std::fs::read_to_string(f).unwrap();
        std::fs::remove_file(f).ok();
        assert!(
            !c.contains("shape=ellipse"),
            "Gateway VNet with no LNG must NOT have external ellipse node:\n{c}"
        );
        assert!(c.contains("hub-vnet"), "hub-vnet must still appear:\n{c}");
        assert!(
            c.contains("GatewaySubnet"),
            "GatewaySubnet must appear in VNet label:\n{c}"
        );
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
        write_peering_dot(&[], &data, &[], &[], f).unwrap();
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
