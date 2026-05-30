//! Mermaid peering diagram generator.
//!
//! Writes a `subnets-YYYY-MM-DD-peering.md` file containing a Mermaid `graph TD`
//! showing VNet peering topology grouped into Subscription Islands.

use crate::azure::{Data, PeeringEdge};
use std::collections::{HashMap, HashSet, VecDeque};
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
    // --- 1. Build VNet metadata from subnet data ---
    let mut vnet_meta: HashMap<String, VNetMeta> = HashMap::new();
    for s in &subnets.data {
        let entry = vnet_meta.entry(s.vnet_name.clone()).or_insert_with(|| VNetMeta {
            subscription_name: s.subscription_name.clone(),
            vnet_cidr: s.vnet_cidr.iter().map(|c| c.to_string()).collect(),
            has_gateway: false,
        });
        if s.subnet_name == "GatewaySubnet" {
            entry.has_gateway = true;
        }
    }

    // --- 2. Supplement from peering edges (VNets not represented in subnet data) ---
    for edge in edges {
        vnet_meta.entry(edge.vnet_name.clone()).or_insert_with(|| VNetMeta {
            subscription_name: edge.subscription_name.clone(),
            vnet_cidr: edge.vnet_cidr.clone(),
            has_gateway: false,
        });
        let remote = edge.remote_vnet_name().to_string();
        if !remote.is_empty() {
            vnet_meta.entry(remote).or_insert_with(|| VNetMeta {
                subscription_name: String::new(),
                vnet_cidr: Vec::new(),
                has_gateway: false,
            });
        }
    }

    // --- 3. Categorise edges into bidir-Connected pairs vs broken ---
    let mut connection_counts: HashMap<(String, String), usize> = HashMap::new();
    for edge in edges.iter().filter(|e| e.is_connected()) {
        let remote = edge.remote_vnet_name().to_string();
        if remote.is_empty() {
            continue;
        }
        let pair = canonical_pair(&edge.vnet_name, &remote);
        *connection_counts.entry(pair).or_insert(0) += 1;
    }
    // Fully bidir = count == 2 (A→B Connected AND B→A Connected)
    let bidir_pairs: HashSet<(String, String)> = connection_counts
        .into_iter()
        .filter(|(_, c)| *c == 2)
        .map(|(pair, _)| pair)
        .collect();

    // Broken = any edge whose canonical pair is NOT bidir
    let broken_edges: Vec<&PeeringEdge> = edges
        .iter()
        .filter(|e| {
            let remote = e.remote_vnet_name().to_string();
            if remote.is_empty() {
                return false;
            }
            !bidir_pairs.contains(&canonical_pair(&e.vnet_name, &remote))
        })
        .collect();

    // --- 4. Find connected components via BFS over bidir pairs only ---
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for (a, b) in &bidir_pairs {
        adjacency.entry(a.clone()).or_default().push(b.clone());
        adjacency.entry(b.clone()).or_default().push(a.clone());
    }

    let mut island_id: HashMap<String, usize> = HashMap::new();
    let mut next_island: usize = 0;
    let mut all_vnets: Vec<String> = vnet_meta.keys().cloned().collect();
    all_vnets.sort(); // deterministic BFS start order

    for vnet in &all_vnets {
        if island_id.contains_key(vnet) {
            continue;
        }
        let mut queue = VecDeque::new();
        queue.push_back(vnet.clone());
        while let Some(v) = queue.pop_front() {
            if island_id.contains_key(&v) {
                continue;
            }
            island_id.insert(v.clone(), next_island);
            if let Some(neighbors) = adjacency.get(&v) {
                for n in neighbors {
                    if !island_id.contains_key(n) {
                        queue.push_back(n.clone());
                    }
                }
            }
        }
        next_island += 1;
    }

    // --- 5. Group VNets by island ---
    let mut islands: HashMap<usize, Vec<String>> = HashMap::new();
    for (vnet, id) in &island_id {
        islands.entry(*id).or_default().push(vnet.clone());
    }
    for vnets in islands.values_mut() {
        vnets.sort();
    }

    // --- 6. Write Mermaid markdown ---
    let file = File::create(filename)?;
    let mut w = BufWriter::new(file);
    let date = chrono::Local::now().format("%Y-%m-%d");

    writeln!(w, "# Azure VNet Peering Diagram")?;
    writeln!(w)?;
    writeln!(w, "Generated: {date}")?;
    writeln!(w)?;
    writeln!(w, "```mermaid")?;
    writeln!(w, "graph TD")?;

    let mut link_index: usize = 0;
    let mut broken_link_indices: Vec<usize> = Vec::new();

    let mut sorted_island_ids: Vec<usize> = islands.keys().cloned().collect();
    sorted_island_ids.sort();

    for island_num in &sorted_island_ids {
        let vnets = &islands[island_num];
        writeln!(w, "    subgraph \"Island {}\"", island_num + 1)?;

        for vnet in vnets {
            let meta = vnet_meta.get(vnet);
            let sub = meta.map(|m| m.subscription_name.as_str()).unwrap_or("?");
            let cidr = meta.map(|m| m.vnet_cidr.join(", ")).unwrap_or_default();
            let node_id = mermaid_id(vnet);
            writeln!(w, "        {node_id}[\"{sub}/{vnet}\\n{cidr}\"]")?;

            if meta.map(|m| m.has_gateway).unwrap_or(false) {
                let ext_id = format!("{node_id}_ext");
                writeln!(w, "        {ext_id}((\"🌐 External / On-Premises\"))")?;
                writeln!(w, "        {node_id} --- {ext_id}")?;
                link_index += 1;
            }
        }

        // Bidir edges for this island
        let mut emitted_bidir: HashSet<(String, String)> = HashSet::new();
        for (a, b) in &bidir_pairs {
            let ia = island_id.get(a).copied().unwrap_or(usize::MAX);
            let ib = island_id.get(b).copied().unwrap_or(usize::MAX);
            if ia != *island_num || ib != *island_num {
                continue;
            }
            let pair = canonical_pair(a, b);
            if emitted_bidir.contains(&pair) {
                continue;
            }
            emitted_bidir.insert(pair);
            writeln!(w, "        {} <--> {}", mermaid_id(a), mermaid_id(b))?;
            link_index += 1;
        }

        writeln!(w, "    end")?;
    }

    // Broken edges (rendered outside subgraphs so they can span island boundaries)
    let mut emitted_broken: HashSet<(String, String)> = HashSet::new();
    for edge in &broken_edges {
        let remote = edge.remote_vnet_name().to_string();
        if remote.is_empty() {
            continue;
        }
        let pair = canonical_pair(&edge.vnet_name, &remote);
        if emitted_broken.contains(&pair) {
            continue;
        }
        emitted_broken.insert(pair);
        // Arrow from the Connected side (or edge.vnet_name if neither / both broken)
        let other_connected = broken_edges
            .iter()
            .any(|e| e.vnet_name == remote && e.remote_vnet_name() == edge.vnet_name && e.is_connected());
        let (from, to) = if other_connected && !edge.is_connected() {
            (remote.as_str(), edge.vnet_name.as_str())
        } else {
            (edge.vnet_name.as_str(), remote.as_str())
        };
        writeln!(w, "    {} --x {}", mermaid_id(from), mermaid_id(to))?;
        broken_link_indices.push(link_index);
        link_index += 1;
    }

    for idx in broken_link_indices {
        writeln!(w, "    linkStyle {idx} stroke:#ff0000,color:#ff0000")?;
    }

    writeln!(w, "```")?;
    Ok(())
}

// ─── helpers ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct VNetMeta {
    subscription_name: String,
    vnet_cidr: Vec<String>,
    has_gateway: bool,
}

/// Sanitise a VNet name into a valid Mermaid node identifier.
fn mermaid_id(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("n_{s}")
}

/// Return a canonical (alphabetically-first, alphabetically-second) pair for deduplication.
fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
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
        Data { data: vec![], count: 0, skip_token: None, total_records: None }
    }

    // --- Cycle 3e: standalone VNet (no peerings) appears in its own subgraph ---
    #[test]
    fn standalone_vnet_gets_own_subgraph() {
        use crate::models::Subnet;
        let mut s = Subnet::default();
        s.vnet_name = "standalone-vnet".into();
        s.subnet_name = "default".into();
        s.subscription_name = "Standalone Sub".into();
        let data = Data { data: vec![s], count: 1, skip_token: None, total_records: None };
        let filename = "/tmp/test-peering-standalone.md";
        write_peering_diagram(&[], &data, filename).unwrap();
        let content = std::fs::read_to_string(filename).unwrap();
        std::fs::remove_file(filename).ok();
        assert!(content.contains("subgraph"), "Standalone VNet must appear in a subgraph:\n{content}");
        assert!(
            content.contains("standalone-vnet"),
            "Standalone VNet must appear in diagram:\n{content}"
        );
        assert!(
            content.contains("Standalone Sub/standalone-vnet"),
            "Standalone VNet must have correct label:\n{content}"
        );
    }

    // --- Cycle 3d: Gateway VNet gets an external connectivity node ---
    #[test]
    fn gateway_vnet_gets_external_node() {
        use crate::models::Subnet;
        let mut s = Subnet::default();
        s.vnet_name = "hub-vnet".into();
        s.subnet_name = "GatewaySubnet".into();
        s.subscription_name = "Prod Sub".into();
        let data = Data { data: vec![s], count: 1, skip_token: None, total_records: None };
        let filename = "/tmp/test-peering-gateway.md";
        write_peering_diagram(&[], &data, filename).unwrap();
        let content = std::fs::read_to_string(filename).unwrap();
        std::fs::remove_file(filename).ok();
        assert!(
            content.contains("On-Premises") || content.contains("External"),
            "Gateway VNet must have external node:\n{content}"
        );
        assert!(content.contains("hub-vnet"), "hub-vnet must appear in diagram:\n{content}");
    }

    // --- Cycle 3c: node labels use Subscription/VNetName format ---
    #[test]
    fn node_labels_include_subscription_slash_vnet() {
        let edges = vec![PeeringEdge {
            vnet_name: "my-vnet".into(),
            subscription_name: "My Sub".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "other-vnet"),
            ..Default::default()
        }];
        let filename = "/tmp/test-peering-label.md";
        write_peering_diagram(&edges, &empty_data(), filename).unwrap();
        let content = std::fs::read_to_string(filename).unwrap();
        std::fs::remove_file(filename).ok();
        assert!(
            content.contains("My Sub/my-vnet"),
            "Node label must be Subscription/VNet:\n{content}"
        );
    }

    // --- Cycle 3b: asymmetric/broken peering produces --x arrow with red styling ---
    #[test]
    fn asymmetric_peering_produces_stop_arrow_in_red() {
        let edges = vec![PeeringEdge {
            vnet_name: "broken-vnet".into(),
            subscription_name: "Sub A".into(),
            peering_state: "Initiated".into(),
            remote_vnet_id: arm_id("s2", "spoke-vnet"),
            ..Default::default()
        }];
        let filename = "/tmp/test-peering-broken.md";
        write_peering_diagram(&edges, &empty_data(), filename).unwrap();
        let content = std::fs::read_to_string(filename).unwrap();
        std::fs::remove_file(filename).ok();
        assert!(content.contains("--x"), "Expected --x stop arrow for broken peering:\n{content}");
        assert!(
            content.contains("stroke:#ff0000"),
            "Expected red stroke for broken peering:\n{content}"
        );
    }

    // --- Cycle 3a: bidirectional Connected pair produces <--> arrow ---
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
        let filename = "/tmp/test-peering-bidir.md";
        write_peering_diagram(&edges, &empty_data(), filename).unwrap();
        let content = std::fs::read_to_string(filename).unwrap();
        std::fs::remove_file(filename).ok();
        assert!(content.contains("<-->"), "Expected <--> for fully connected peers:\n{content}");
    }
}
