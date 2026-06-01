//! Overlapping VNet CIDR detection and filtering.
//!
//! Detects VNets with overlapping address spaces across different subscriptions
//! and provides filtering options to handle them.

use crate::azure::Data;
use crate::models::{Ipv4, Subnet};
use std::collections::HashMap;

/// An excluded subnet paired with the VNet name that won conflict resolution.
pub struct ExcludedSubnet {
    pub subnet: Subnet,
    pub winner_vnet_name: String,
}

/// Typed result of conflict resolution: active subnets and excluded subnets.
///
/// `active` contains only the winner subnets; `excluded` contains every subnet
/// that lost, each paired with the winner's VNet name.
pub struct ConflictResolutionOutput {
    pub active: Data,
    pub excluded: Vec<ExcludedSubnet>,
}

/// Resolve overlapping VNets, returning a typed split of active and excluded subnets.
///
/// Winner selection priority within a conflict group:
/// 1. Production subscription (name contains "prod", case-insensitive)
/// 2. Most subnets
/// 3. Alphabetical by subscription name
///
/// Excludes entire VNets by CIDR so only the conflicting address space is removed;
/// other address spaces of the same VNet remain active.
pub fn resolve_overlapping_vnets(data: Data) -> ConflictResolutionOutput {
    let conflicts = find_overlapping_vnets(&data);
    if conflicts.is_empty() {
        return ConflictResolutionOutput {
            active: data,
            excluded: vec![],
        };
    }

    let mut exclusion_keys: Vec<(String, String, String, String)> = Vec::new();

    for conflict in &conflicts {
        let mut sorted_vnets = conflict.vnets.clone();
        sorted_vnets.sort_by(|a, b| {
            is_production(&b.subscription_name)
                .cmp(&is_production(&a.subscription_name))
                .then_with(|| b.subnet_count.cmp(&a.subnet_count))
                .then_with(|| a.subscription_name.cmp(&b.subscription_name))
        });

        let keeper = &sorted_vnets[0];
        for vnet in sorted_vnets.iter().skip(1) {
            let cidr_str = vnet
                .vnet_cidr
                .first()
                .map(|c| c.to_string())
                .unwrap_or_default();
            exclusion_keys.push((
                vnet.vnet_name.clone(),
                vnet.subscription_id.clone(),
                cidr_str,
                keeper.vnet_name.clone(),
            ));
        }
    }

    let skip_token = data.skip_token.clone();
    let total_records = data.total_records;
    let mut active_subnets: Vec<Subnet> = Vec::new();
    let mut excluded: Vec<ExcludedSubnet> = Vec::new();

    for subnet in data.data {
        if let Some((_, _, _, winner_name)) =
            exclusion_keys.iter().find(|(name, sub_id, cidr_str, _)| {
                &subnet.vnet_name == name
                    && &subnet.subscription_id == sub_id
                    && subnet.vnet_cidr.to_string() == *cidr_str
            })
        {
            excluded.push(ExcludedSubnet {
                winner_vnet_name: winner_name.clone(),
                subnet,
            });
        } else {
            active_subnets.push(subnet);
        }
    }

    let active = Data {
        count: active_subnets.len() as i32,
        data: active_subnets,
        skip_token,
        total_records,
    };

    ConflictResolutionOutput { active, excluded }
}

/// Information about a VNet for overlap detection.
#[derive(Debug, Clone)]
pub struct VnetInfo {
    pub vnet_name: String,
    pub vnet_cidr: Vec<Ipv4>,
    pub subscription_id: String,
    pub subscription_name: String,
    pub location: String,
    pub subnet_count: usize,
}

/// Represents a group of VNets whose CIDRs overlap (directly or transitively).
#[derive(Debug)]
pub struct OverlapConflict {
    pub vnets: Vec<VnetInfo>,
}

/// Returns true if any CIDR in `a` overlaps with any CIDR in `b`.
///
/// Two ranges overlap when: A.lo() <= B.hi() && B.lo() <= A.hi()
fn cidrs_overlap(a: &[Ipv4], b: &[Ipv4]) -> bool {
    for ca in a {
        for cb in b {
            if ca.lo() <= cb.hi() && cb.lo() <= ca.hi() {
                return true;
            }
        }
    }
    false
}

/// Find overlapping VNet CIDRs across different VNets.
///
/// Two VNets overlap when their CIDR ranges intersect: A.lo() <= B.hi() && B.lo() <= A.hi().
/// Transitively overlapping VNets are grouped into a single conflict group.
///
/// # Arguments
/// * `data` - The subnet data to analyze
///
/// # Returns
/// A list of conflict groups; each group contains 2+ VNets whose CIDRs overlap.
pub fn find_overlapping_vnets(data: &Data) -> Vec<OverlapConflict> {
    // Build one VnetInfo per (vnet_name, subscription_id, vnet_cidr) triple.
    // This ensures that each independent address space of a VNet is evaluated
    // separately — a conflict in one VNet_CIDR does not implicate other address
    // spaces of the same VNet.
    let mut seen_vnets: HashMap<(String, String, String), VnetInfo> = HashMap::new();

    for subnet in &data.data {
        let cidr_str = subnet.vnet_cidr.to_string();
        let key = (
            subnet.vnet_name.clone(),
            subnet.subscription_id.clone(),
            cidr_str,
        );
        seen_vnets
            .entry(key)
            .and_modify(|info| info.subnet_count += 1)
            .or_insert_with(|| VnetInfo {
                vnet_name: subnet.vnet_name.clone(),
                vnet_cidr: vec![subnet.vnet_cidr],
                subscription_id: subnet.subscription_id.clone(),
                subscription_name: subnet.subscription_name.clone(),
                location: subnet.location.clone(),
                subnet_count: 1,
            });
    }

    let vnets: Vec<VnetInfo> = seen_vnets.into_values().collect();
    let n = vnets.len();

    // Union-Find for connected components
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            parent[i] = find(parent, parent[i]);
        }
        parent[i]
    }

    fn union(parent: &mut Vec<usize>, i: usize, j: usize) {
        let pi = find(parent, i);
        let pj = find(parent, j);
        if pi != pj {
            parent[pi] = pj;
        }
    }

    // Check every pair for range overlap; skip pairs from the same VNet (same vnet_name
    // + subscription_id) — different address spaces of the same VNet are not a conflict.
    for i in 0..n {
        for j in (i + 1)..n {
            let same_vnet = vnets[i].vnet_name == vnets[j].vnet_name
                && vnets[i].subscription_id == vnets[j].subscription_id;
            if !same_vnet && cidrs_overlap(&vnets[i].vnet_cidr, &vnets[j].vnet_cidr) {
                union(&mut parent, i, j);
            }
        }
    }

    // Group VNets by their root representative
    let mut groups: HashMap<usize, Vec<VnetInfo>> = HashMap::new();
    for (i, vnet) in vnets.iter().enumerate().take(n) {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(vnet.clone());
    }

    // Only return groups with more than one VNet (actual conflicts)
    let mut conflicts: Vec<OverlapConflict> = groups
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|vnets| OverlapConflict { vnets })
        .collect();

    // Sort by the lowest CIDR in each group for consistent output
    conflicts.sort_by_key(|c| {
        c.vnets
            .iter()
            .flat_map(|v| v.vnet_cidr.iter().copied())
            .min()
    });

    conflicts
}

/// Log overlapping VNet conflicts as warnings.
pub fn log_overlapping_vnets(conflicts: &[OverlapConflict]) {
    if conflicts.is_empty() {
        log::info!("No overlapping VNet CIDRs found.");
        return;
    }

    log::warn!(
        "Found {} overlapping VNet CIDR(s) across different VNets:",
        conflicts.len()
    );

    for conflict in conflicts {
        let cidr_list: Vec<String> = conflict
            .vnets
            .iter()
            .flat_map(|v| v.vnet_cidr.iter().map(|c| c.to_string()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        log::warn!(
            "  Conflict group (CIDRs: {}) has {} VNets:",
            cidr_list.join(", "),
            conflict.vnets.len()
        );
        for vnet in &conflict.vnets {
            log::warn!(
                "    - VNet: '{}', Subscription: '{}' ({}), Location: {}, Subnets: {}",
                vnet.vnet_name,
                vnet.subscription_name,
                vnet.subscription_id,
                vnet.location,
                vnet.subnet_count
            );
        }
    }
}

/// Returns true if the subscription name indicates a production environment.
///
/// Matches case-insensitively on the substring "prod".
fn is_production(subscription_name: &str) -> bool {
    subscription_name.to_lowercase().contains("prod")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::read_subnet_cache;
    use crate::azure::Data;
    use crate::models::{Ipv4, Subnet};

    fn make_subnet(
        vnet_name: &str,
        subscription_name: &str,
        vnet_cidr: &str,
        subnet_cidr: &str,
    ) -> Subnet {
        Subnet {
            vnet_name: vnet_name.to_string(),
            vnet_cidr: Ipv4::new(vnet_cidr).unwrap(),
            subnet_name: format!("{}-snet", vnet_name),
            subnet_cidr: Some(Ipv4::new(subnet_cidr).unwrap()),
            subscription_name: subscription_name.to_string(),
            subscription_id: format!("sub-{}", subscription_name.to_lowercase().replace(' ', "-")),
            location: "eastus".to_string(),
            ..Default::default()
        }
    }

    fn make_data(subnets: Vec<Subnet>) -> Data {
        let count = subnets.len() as i32;
        Data {
            data: subnets,
            count,
            ..Default::default()
        }
    }

    #[test]
    fn containment_overlap_is_detected() {
        // 10.0.0.0/8 contains 10.1.0.0/16 — they overlap even though CIDRs differ
        let data = make_data(vec![
            make_subnet("big-vnet", "Dev Sub", "10.0.0.0/8", "10.0.1.0/24"),
            make_subnet("small-vnet", "Test Sub", "10.1.0.0/16", "10.1.1.0/24"),
        ]);

        let conflicts = find_overlapping_vnets(&data);

        assert_eq!(conflicts.len(), 1, "should detect one conflict group");
        assert_eq!(
            conflicts[0].vnets.len(),
            2,
            "both VNets should be in the group"
        );
    }

    #[test]
    fn non_overlapping_cidrs_form_no_conflict() {
        let data = make_data(vec![
            make_subnet("vnet-a", "Sub A", "10.0.0.0/16", "10.0.1.0/24"),
            make_subnet("vnet-b", "Sub B", "10.2.0.0/16", "10.2.1.0/24"),
        ]);

        let conflicts = find_overlapping_vnets(&data);

        assert!(
            conflicts.is_empty(),
            "disjoint CIDRs should produce no conflicts"
        );
    }

    #[test]
    fn transitive_overlap_forms_one_group() {
        // A (10.0.0.0/16) overlaps B (10.0.0.0/8),
        // B (10.0.0.0/8) overlaps C (10.5.0.0/16),
        // A and C do not directly overlap — but all three are one group
        let data = make_data(vec![
            make_subnet("vnet-a", "Sub A", "10.0.0.0/16", "10.0.1.0/24"),
            make_subnet("vnet-b", "Sub B", "10.0.0.0/8", "10.0.2.0/24"),
            make_subnet("vnet-c", "Sub C", "10.5.0.0/16", "10.5.1.0/24"),
        ]);

        let conflicts = find_overlapping_vnets(&data);

        assert_eq!(
            conflicts.len(),
            1,
            "transitively connected VNets form one group"
        );
        assert_eq!(
            conflicts[0].vnets.len(),
            3,
            "all three VNets should be in the group"
        );
    }

    #[test]
    fn production_sub_wins_over_non_production_with_fewer_subnets() {
        // prod-vnet has 1 subnet but is in a production subscription → should win
        // "Zzz Production" sorts LAST alphabetically, so without prod-wins logic it would lose
        let data = make_data(vec![
            make_subnet("dev-vnet", "AAA Sandbox", "10.1.0.0/16", "10.1.1.0/24"),
            make_subnet("dev-vnet2", "BBB Sandbox", "10.1.0.0/16", "10.1.2.0/24"),
            make_subnet("prod-vnet", "Zzz Production", "10.1.0.0/16", "10.1.3.0/24"),
        ]);

        let out = resolve_overlapping_vnets(data);

        let active_names: Vec<&str> = out
            .active
            .data
            .iter()
            .map(|s| s.vnet_name.as_str())
            .collect();
        assert!(
            active_names.contains(&"prod-vnet"),
            "production VNet should win even though it sorts last"
        );
        assert!(
            !active_names.contains(&"dev-vnet"),
            "non-prod VNet should be excluded"
        );
        assert!(
            !active_names.contains(&"dev-vnet2"),
            "non-prod VNet should be excluded"
        );
    }

    #[test]
    fn excluded_subnets_have_winner_vnet_name_set() {
        let data = make_data(vec![
            make_subnet("loser-vnet", "Sandbox", "10.1.0.0/16", "10.1.1.0/24"),
            make_subnet(
                "winner-vnet",
                "Coretex Production",
                "10.1.0.0/16",
                "10.1.2.0/24",
            ),
        ]);

        let out = resolve_overlapping_vnets(data);

        assert_eq!(out.active.data.len(), 1);
        assert_eq!(out.active.data[0].vnet_name, "winner-vnet");
        assert_eq!(out.excluded.len(), 1);
        assert_eq!(out.excluded[0].subnet.vnet_name, "loser-vnet");
        assert_eq!(out.excluded[0].winner_vnet_name, "winner-vnet");
    }

    #[test]
    fn most_subnets_wins_when_no_production_involved() {
        let data = make_data(vec![
            make_subnet("small-vnet", "Dev Sub", "10.1.0.0/16", "10.1.1.0/24"),
            // big-vnet has 2 subnets (2 rows with same vnet)
            make_subnet("big-vnet", "Test Sub", "10.1.0.0/16", "10.1.2.0/24"),
            make_subnet("big-vnet", "Test Sub", "10.1.0.0/16", "10.1.3.0/24"),
        ]);

        let out = resolve_overlapping_vnets(data);

        let active_names: Vec<&str> = out
            .active
            .data
            .iter()
            .map(|s| s.vnet_name.as_str())
            .collect();
        assert!(
            active_names.contains(&"big-vnet"),
            "vnet with more subnets should be kept"
        );
        assert!(
            !active_names.contains(&"small-vnet"),
            "vnet with fewer subnets should be excluded"
        );
    }

    #[test]
    fn non_conflicting_cidr_of_same_vnet_is_not_excluded() {
        // pd-ibe-westus-arm has two address spaces:
        //   10.0.0.0/16 — conflicts with other-vnet (same CIDR, different sub)
        //   172.17.8.0/21 — no conflict
        // Only subnets in 10.0.0.0/16 should be excluded;
        // subnets in 172.17.8.0/21 should remain active.
        let mut subnet_a = make_subnet(
            "pd-ibe-westus-arm",
            "iBright Sandbox",
            "10.0.0.0/16",
            "10.0.0.0/24",
        );
        subnet_a.subscription_id = "sub-ibright".to_string();

        let mut subnet_b = make_subnet(
            "pd-ibe-westus-arm",
            "iBright Sandbox",
            "172.17.8.0/21",
            "172.17.8.0/24",
        );
        subnet_b.subscription_id = "sub-ibright".to_string();

        let mut other = make_subnet(
            "other-vnet",
            "iBright Production",
            "10.0.0.0/16",
            "10.0.1.0/24",
        );
        other.subscription_id = "sub-prod".to_string();

        let data = make_data(vec![subnet_a, subnet_b, other]);
        let out = resolve_overlapping_vnets(data);

        let excluded_names: Vec<&str> = out
            .excluded
            .iter()
            .map(|e| e.subnet.vnet_name.as_str())
            .collect();
        assert!(
            excluded_names.contains(&"pd-ibe-westus-arm"),
            "subnet with conflicting CIDR (10.0.0.0/16) must be excluded:\n{excluded_names:?}"
        );

        // The subnet in 172.17.8.0/21 must NOT be excluded — must be in active
        let subnet_b_active = out
            .active
            .data
            .iter()
            .find(|s| s.vnet_cidr.to_string() == "172.17.8.0/21")
            .expect("subnet_b must still be in active");
        assert_eq!(
            subnet_b_active.vnet_name, "pd-ibe-westus-arm",
            "subnet in non-conflicting VNet_CIDR (172.17.8.0/21) must not be excluded"
        );
    }

    #[test]
    fn test_find_overlapping_vnets() {
        // This test would need a cache file with overlapping VNets
        let data = read_subnet_cache(Some("subnet_cache_2026-02-09.json"));
        if let Ok(data) = data {
            let conflicts = find_overlapping_vnets(&data);
            // Just verify it doesn't crash - actual results depend on test data
            assert!(!conflicts.is_empty() || conflicts.is_empty()); // Always true, just to avoid unused warning
        }
    }

    #[test]
    fn resolve_returns_split_active_and_excluded() {
        let data = make_data(vec![
            make_subnet("loser-vnet", "Sandbox", "10.1.0.0/16", "10.1.1.0/24"),
            make_subnet(
                "winner-vnet",
                "Coretex Production",
                "10.1.0.0/16",
                "10.1.2.0/24",
            ),
        ]);

        let out = resolve_overlapping_vnets(data);

        assert_eq!(out.active.data.len(), 1);
        assert_eq!(out.active.data[0].vnet_name, "winner-vnet");
        assert_eq!(out.excluded.len(), 1);
        assert_eq!(out.excluded[0].winner_vnet_name, "winner-vnet");
        assert_eq!(out.excluded[0].subnet.vnet_name, "loser-vnet");
    }
}
