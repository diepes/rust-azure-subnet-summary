//! Shared topology analysis for peering diagram writers.
//!
//! Parses raw `PeeringEdge` + subnet `Data` into a `PeeringTopology` that both
//! the Mermaid and Graphviz DOT writers consume.

use crate::azure::{Data, LocalGatewayRow, PeeringEdge, VWanRow};
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
    /// Names of on-premises Local Network Gateways connected to this VNet (empty if none).
    pub on_prem_names: Vec<String>,
    /// Deduplicated on-premises CIDRs from all connected Local Network Gateways.
    pub on_prem_cidrs: Vec<String>,
    /// Name of the VPN Gateway resource hosted in this VNet's GatewaySubnet (None if not a Gateway VNet).
    pub vng_name: Option<String>,
    /// BGP ASN of the Azure VPN Gateway (None if BGP disabled or not a Gateway VNet).
    pub vng_bgp_asn: Option<String>,
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

/// Metadata for a Virtual WAN Hub node.
#[derive(Debug, Clone)]
pub(super) struct VWanHub {
    /// Hub resource name (e.g. `p-aue-platform-hub`).
    pub hub_name: String,
    /// Hub address prefix / CIDR (e.g. `10.100.0.0/23`).
    pub hub_address_prefix: String,
    /// Parent Virtual WAN resource name.
    pub virtual_wan_name: String,
    /// Sorted list of spoke VNet names connected to this hub.
    pub spoke_vnets: Vec<String>,
    /// `true` when the hub was confirmed in the vWAN cache (both sides of the
    /// connection are known). `false` means it was derived from HV_ peering
    /// edges only — the hub may have been deleted or is outside our query scope.
    pub validated: bool,
    /// Azure subscription that owns the hub (from vWAN cache; empty if unvalidated).
    pub subscription_name: String,
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
    /// vWAN Hub nodes (rendered outside all islands).
    pub vwan_hubs: Vec<VWanHub>,
    /// spoke VNet name → hub name (for edge rendering).
    #[allow(dead_code)]
    pub vwan_spoke_to_hub: HashMap<String, String>,
}

// ─── builder ─────────────────────────────────────────────────────────────────

/// Build a `PeeringTopology` from raw Azure data.
pub(super) fn build_topology(
    edges: &[PeeringEdge],
    subnets: &Data,
    local_gateways: &[LocalGatewayRow],
    vwan: &[VWanRow],
) -> PeeringTopology {
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
                on_prem_names: Vec::new(),
                on_prem_cidrs: Vec::new(),
                vng_name: None,
                vng_bgp_asn: None,
            });
        if s.subnet_name == "GatewaySubnet" {
            entry.has_gateway = true;
        }
    }

    // --- 2. Fill in VNets that only appear in peering edges ---
    // Skip Azure-internal vWAN Hub VNets (HV_ prefix) — they are handled separately.
    for edge in edges {
        vnet_meta
            .entry(edge.vnet_name.clone())
            .or_insert_with(|| VNetMeta {
                subscription_name: edge.subscription_name.clone(),
                vnet_cidr: edge.vnet_cidr.clone(),
                has_gateway: false,
                // VNet exists — it reported its own peering configuration.
                // Only the *remote* target may be a phantom reference.
                missing: false,
                on_prem_names: Vec::new(),
                on_prem_cidrs: Vec::new(),
                vng_name: None,
                vng_bgp_asn: None,
            });
        let remote = edge.remote_vnet_name().to_string();
        // HV_* names are Azure-managed vWAN Hub fabric VNets — not real queryable VNets.
        if !remote.is_empty() && !remote.starts_with("HV_") {
            vnet_meta.entry(remote).or_insert_with(|| VNetMeta {
                // Best available identifier for a remote-only VNet is the sub ID from the ARM path
                subscription_name: edge.remote_subscription_id().to_string(),
                vnet_cidr: Vec::new(),
                has_gateway: false,
                missing: true,
                on_prem_names: Vec::new(),
                on_prem_cidrs: Vec::new(),
                vng_name: None,
                vng_bgp_asn: None,
            });
        }
    }

    // --- 3. Populate on-premises info from Local Network Gateways ---
    for row in local_gateways {
        if let Some(meta) = vnet_meta.get_mut(&row.vnet_name) {
            if !row.local_gw_name.is_empty()
                && !meta.on_prem_names.contains(&row.local_gw_name)
            {
                meta.on_prem_names.push(row.local_gw_name.clone());
            }
            for cidr in &row.address_prefixes {
                if !cidr.is_empty() && !meta.on_prem_cidrs.contains(cidr) {
                    meta.on_prem_cidrs.push(cidr.clone());
                }
            }
            // Populate VNG fields from the first row that has them (same VNG across all LNG rows).
            if meta.vng_name.is_none() && !row.vng_name.is_empty() {
                meta.vng_name = Some(row.vng_name.clone());
            }
            if meta.vng_bgp_asn.is_none() && !row.vng_bgp_asn.is_empty() {
                meta.vng_bgp_asn = Some(row.vng_bgp_asn.clone());
            }
        }
    }

    // --- 4. Populate vWAN hub topology ---
    // Primary source: extract hub name from HV_<hub-name>_<uuid> remote VNet IDs in
    // peering edges. This works even when the vWAN Resource Graph query returns nothing
    // (the hubvirtualnetworkconnections resource type is not always indexed in ARG).
    // vwan rows are used as an enrichment source for hub_address_prefix / virtual_wan_name.
    let uuid_suffix = regex::Regex::new(
        r"_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
    )
    .expect("static regex");

    let mut vwan_spoke_to_hub: HashMap<String, String> = HashMap::new();
    let mut hub_map: std::collections::BTreeMap<String, VWanHub> =
        std::collections::BTreeMap::new();

    // Step 4a — derive from HV_ peering edges (always available)
    for edge in edges {
        let remote = edge.remote_vnet_name().to_string();
        if remote.starts_with("HV_") && edge.is_connected() {
            let stripped = &remote[3..]; // drop "HV_" prefix
            let hub_name = uuid_suffix.replace(stripped, "").to_string();
            if hub_name.is_empty() {
                continue;
            }
            vwan_spoke_to_hub
                .entry(edge.vnet_name.clone())
                .or_insert_with(|| hub_name.clone());
            let hub = hub_map.entry(hub_name.clone()).or_insert_with(|| VWanHub {
                hub_name: hub_name.clone(),
                hub_address_prefix: String::new(),
                virtual_wan_name: String::new(),
                spoke_vnets: Vec::new(),
                validated: false,
                subscription_name: String::new(),
            });
            if !hub.spoke_vnets.contains(&edge.vnet_name) {
                hub.spoke_vnets.push(edge.vnet_name.clone());
            }
        }
    }

    // Step 4b — enrich hub metadata from vWAN cache rows (adds CIDR + vWAN name).
    // The ARG query returns one row per hub (not per spoke), so only hub-level
    // fields are available here; spoke connections come from step 4a (HV_ edges).
    //
    // Azure truncates the hub name in the HV_<name>_<uuid> peering to ~22 chars,
    // so the key in hub_map from step 4a may be a prefix of the full name from
    // the vWAN cache.  We reconcile by renaming the truncated key to the full name.
    for row in vwan {
        // Look for an existing entry whose key is a prefix of the full hub name
        // (or vice versa — the full name is a prefix of the stored key, unlikely
        // but handled for symmetry).
        let truncated_key: Option<String> = hub_map
            .keys()
            .find(|k| {
                let k_lc = k.to_lowercase().replace('-', "_");
                let r_lc = row.hub_name.to_lowercase().replace('-', "_");
                r_lc.starts_with(&k_lc) || k_lc.starts_with(&r_lc)
            })
            .filter(|k| *k != &row.hub_name)
            .cloned();

        if let Some(old_key) = truncated_key {
            // Rename the truncated-name entry to the full canonical name and
            // merge its spoke list, then continue with the full-name key.
            let mut hub = hub_map.remove(&old_key).expect("key just found");
            hub.hub_name = row.hub_name.clone();
            hub_map.insert(row.hub_name.clone(), hub);
        }

        // Ensure the full-name entry exists (may have been just renamed above).
        hub_map.entry(row.hub_name.clone()).or_insert_with(|| VWanHub {
            hub_name: row.hub_name.clone(),
            hub_address_prefix: row.hub_address_prefix.clone(),
            virtual_wan_name: row.virtual_wan_name.clone(),
            spoke_vnets: Vec::new(),
            validated: false,
            subscription_name: row.subscription_name.clone(),
        });
        let hub = hub_map.get_mut(&row.hub_name).expect("just inserted");
        // The hub exists in the vWAN cache — this confirms the hub side of the
        // peering (equivalent to the second side in a normal VNet peering).
        hub.validated = true;
        if hub.subscription_name.is_empty() && !row.subscription_name.is_empty() {
            hub.subscription_name = row.subscription_name.clone();
        }
        if hub.hub_address_prefix.is_empty() && !row.hub_address_prefix.is_empty() {
            hub.hub_address_prefix = row.hub_address_prefix.clone();
        }
        if hub.virtual_wan_name.is_empty() && !row.virtual_wan_name.is_empty() {
            hub.virtual_wan_name = row.virtual_wan_name.clone();
        }
    }

    for hub in hub_map.values_mut() {
        hub.spoke_vnets.sort();
    }
    let vwan_hubs: Vec<VWanHub> = hub_map.into_values().collect();

    // --- 5. Categorise edges ---
    let mut connection_counts: HashMap<(String, String), usize> = HashMap::new();
    for edge in edges.iter().filter(|e| e.is_connected()) {
        let remote = edge.remote_vnet_name().to_string();
        // Exclude HV_* vWAN fabric VNets from bidir pair counting.
        if !remote.is_empty() && !remote.starts_with("HV_") {
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

    // Broken edges: deduplicated, direction resolved (Connected side is `from`).
    // Edges to HV_* (vWAN Hub fabric) or to known vWAN spokes are skipped — they
    // are rendered separately as vWAN hub edges.
    let mut seen_broken: HashSet<(String, String)> = HashSet::new();
    let mut broken_edges: Vec<BrokenEdge> = Vec::new();
    for edge in edges {
        let remote = edge.remote_vnet_name().to_string();
        if remote.is_empty() || remote.starts_with("HV_") {
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

    // --- 6. Connected components via BFS over bidir pairs ---
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for (a, b) in &bidir_pairs {
        adjacency.entry(a.clone()).or_default().push(b.clone());
        adjacency.entry(b.clone()).or_default().push(a.clone());
    }

    // Add synthetic adjacency between all spokes of the same vWAN hub so they
    // land in the same island. The hub diamond node itself is rendered outside
    // clusters; only the spoke VNets need to be grouped together.
    for hub in &vwan_hubs {
        // Only consider spokes that actually appear in vnet_meta (i.e. have subnet data).
        let known_spokes: Vec<&str> = hub
            .spoke_vnets
            .iter()
            .filter(|s| vnet_meta.contains_key(*s))
            .map(|s| s.as_str())
            .collect();
        // Connect each spoke to the next — a chain is enough to unify them in BFS.
        for window in known_spokes.windows(2) {
            let (a, b) = (window[0], window[1]);
            adjacency
                .entry(a.to_string())
                .or_default()
                .push(b.to_string());
            adjacency
                .entry(b.to_string())
                .or_default()
                .push(a.to_string());
        }
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
        vwan_hubs,
        vwan_spoke_to_hub,
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
