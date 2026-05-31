//! Shared topology analysis for peering diagram writers.
//!
//! Parses raw `PeeringEdge` + subnet `Data` into a `PeeringTopology` that both
//! the Mermaid and Graphviz DOT writers consume.

use crate::azure::{Data, PeeringEdge};
use std::collections::{HashMap, HashSet, VecDeque};

// ─── public types ────────────────────────────────────────────────────────────

/// Metadata collected for a single VNet.
#[derive(Debug, Clone)]
pub(super) struct VNetMeta {
    pub subscription_name: String,
    pub vnet_cidr: Vec<String>,
    pub has_gateway: bool,
    /// `true` when this VNet has no subnet records — it was only seen as a remote
    /// target in a peering edge or as a peering source without subnet data.
    pub missing: bool,
}

/// A resolved broken directed edge `(from, to)`.
///
/// Direction is chosen so that the Connected side (if any) is `from`.
/// For both-broken / one-sided edges the first-seen direction is kept.
#[derive(Debug, Clone)]
pub(super) struct BrokenEdge {
    pub from: String,
    pub to: String,
}

/// Pre-processed peering topology ready for rendering.
pub(super) struct PeeringTopology {
    /// Metadata keyed by VNet name.
    pub vnet_meta: HashMap<String, VNetMeta>,
    /// Canonical `(lo, hi)` pairs where BOTH sides reported `Connected`.
    pub bidir_pairs: HashSet<(String, String)>,
    /// Deduplicated broken directed edges.
    pub broken_edges: Vec<BrokenEdge>,
    /// Sorted islands: `islands[i]` is a sorted `Vec` of VNet names in island `i`.
    pub islands: Vec<Vec<String>>,
    /// VNet name → island index.
    pub island_id: HashMap<String, usize>,
}

// ─── builder ─────────────────────────────────────────────────────────────────

/// Build a `PeeringTopology` from raw Azure data.
pub(super) fn build_topology(edges: &[PeeringEdge], subnets: &Data) -> PeeringTopology {
    // --- 1. VNet metadata from subnet data ---
    let mut vnet_meta: HashMap<String, VNetMeta> = HashMap::new();
    for s in &subnets.data {
        let entry = vnet_meta
            .entry(s.vnet_name.clone())
            .or_insert_with(|| VNetMeta {
                subscription_name: s.subscription_name.clone(),
                vnet_cidr: s.vnet_cidr.iter().map(|c| c.to_string()).collect(),
                has_gateway: false,
                missing: false,
            });
        if s.subnet_name == "GatewaySubnet" {
            entry.has_gateway = true;
        }
    }

    // --- 2. Fill in VNets that only appear in peering edges ---
    for edge in edges {
        vnet_meta
            .entry(edge.vnet_name.clone())
            .or_insert_with(|| VNetMeta {
                subscription_name: edge.subscription_name.clone(),
                vnet_cidr: edge.vnet_cidr.clone(),
                has_gateway: false,
                missing: true,
            });
        let remote = edge.remote_vnet_name().to_string();
        if !remote.is_empty() {
            vnet_meta.entry(remote).or_insert_with(|| VNetMeta {
                // Best available identifier for a remote-only VNet is the sub ID from the ARM path
                subscription_name: edge.remote_subscription_id().to_string(),
                vnet_cidr: Vec::new(),
                has_gateway: false,
                missing: true,
            });
        }
    }

    // --- 3. Categorise edges ---
    let mut connection_counts: HashMap<(String, String), usize> = HashMap::new();
    for edge in edges.iter().filter(|e| e.is_connected()) {
        let remote = edge.remote_vnet_name().to_string();
        if !remote.is_empty() {
            *connection_counts
                .entry(canonical_pair(&edge.vnet_name, &remote))
                .or_insert(0) += 1;
        }
    }
    let bidir_pairs: HashSet<(String, String)> = connection_counts
        .into_iter()
        .filter(|(_, c)| *c == 2)
        .map(|(p, _)| p)
        .collect();

    // Broken edges: deduplicated, direction resolved (Connected side is `from`)
    let mut seen_broken: HashSet<(String, String)> = HashSet::new();
    let mut broken_edges: Vec<BrokenEdge> = Vec::new();
    for edge in edges {
        let remote = edge.remote_vnet_name().to_string();
        if remote.is_empty() {
            continue;
        }
        let pair = canonical_pair(&edge.vnet_name, &remote);
        if bidir_pairs.contains(&pair) || seen_broken.contains(&pair) {
            continue;
        }
        seen_broken.insert(pair);
        // Prefer the direction where the source is Connected
        let other_connected = edges.iter().any(|e| {
            e.vnet_name == remote && e.remote_vnet_name() == edge.vnet_name && e.is_connected()
        });
        let (from, to) = if other_connected && !edge.is_connected() {
            (remote.clone(), edge.vnet_name.clone())
        } else {
            (edge.vnet_name.clone(), remote.clone())
        };
        broken_edges.push(BrokenEdge { from, to });
    }

    // --- 4. Connected components via BFS over bidir pairs ---
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for (a, b) in &bidir_pairs {
        adjacency.entry(a.clone()).or_default().push(b.clone());
        adjacency.entry(b.clone()).or_default().push(a.clone());
    }

    let mut island_id: HashMap<String, usize> = HashMap::new();
    let mut islands: Vec<Vec<String>> = Vec::new();
    let mut all_vnets: Vec<String> = vnet_meta.keys().cloned().collect();
    all_vnets.sort();

    for vnet in &all_vnets {
        if island_id.contains_key(vnet) {
            continue;
        }
        let island_num = islands.len();
        let mut members: Vec<String> = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(vnet.clone());
        while let Some(v) = queue.pop_front() {
            if island_id.contains_key(&v) {
                continue;
            }
            island_id.insert(v.clone(), island_num);
            members.push(v.clone());
            if let Some(neighbors) = adjacency.get(&v) {
                for n in neighbors {
                    if !island_id.contains_key(n) {
                        queue.push_back(n.clone());
                    }
                }
            }
        }
        members.sort();
        islands.push(members);
    }

    PeeringTopology {
        vnet_meta,
        bidir_pairs,
        broken_edges,
        islands,
        island_id,
    }
}

// ─── utilities ───────────────────────────────────────────────────────────────

/// Returns `(lo, hi)` with `lo <= hi` alphabetically — used for deduplication.
pub(super) fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Sanitise a VNet name into a valid Mermaid / DOT node identifier.
pub(super) fn node_id(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("n_{s}")
}
