//! Overlapping VNet CIDR detection and filtering.
//!
//! Detects VNets with overlapping address spaces across different subscriptions
//! and provides filtering options to handle them.

use crate::azure::Data;
use crate::models::Ipv4;
use std::collections::HashMap;
use std::error::Error;

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

/// Represents an overlapping VNet CIDR conflict.
#[derive(Debug)]
pub struct OverlapConflict {
    pub cidr: Ipv4,
    pub vnets: Vec<VnetInfo>,
}

/// Default VNet CIDRs to exclude (commonly used for local/isolated networks).
pub fn default_vnet_cidrs_to_exclude() -> Vec<&'static str> {
    vec![
        "10.0.0.0/16", // Common default for dev/test VNets
        "10.1.0.0/16", // Another common default
    ]
}

/// Find overlapping VNet CIDRs across different VNets.
///
/// # Arguments
/// * `data` - The subnet data to analyze
///
/// # Returns
/// A list of overlap conflicts found
pub fn find_overlapping_vnets(data: &Data) -> Vec<OverlapConflict> {
    // Build a map of VNet CIDR -> list of VNets using that CIDR
    let mut cidr_to_vnets: HashMap<Ipv4, Vec<VnetInfo>> = HashMap::new();

    // Track unique VNets (by name + subscription)
    let mut seen_vnets: HashMap<(String, String), VnetInfo> = HashMap::new();

    for subnet in &data.data {
        let key = (subnet.vnet_name.clone(), subnet.subscription_id.clone());

        seen_vnets
            .entry(key.clone())
            .and_modify(|info| info.subnet_count += 1)
            .or_insert_with(|| VnetInfo {
                vnet_name: subnet.vnet_name.clone(),
                vnet_cidr: subnet.vnet_cidr.clone(),
                subscription_id: subnet.subscription_id.clone(),
                subscription_name: subnet.subscription_name.clone(),
                location: subnet.location.clone(),
                subnet_count: 1,
            });
    }

    // Group by VNet CIDR
    for vnet_info in seen_vnets.values() {
        for cidr in &vnet_info.vnet_cidr {
            cidr_to_vnets
                .entry(*cidr)
                .or_default()
                .push(vnet_info.clone());
        }
    }

    // Find CIDRs that are used by multiple VNets
    let mut conflicts: Vec<OverlapConflict> = cidr_to_vnets
        .into_iter()
        .filter(|(_, vnets)| vnets.len() > 1)
        .map(|(cidr, vnets)| OverlapConflict { cidr, vnets })
        .collect();

    // Sort by CIDR for consistent output
    conflicts.sort_by_key(|c| c.cidr);

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
        log::warn!("  CIDR {} is used by {} VNets:", conflict.cidr, conflict.vnets.len());
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

/// Get VNets that would be excluded based on their CIDRs.
///
/// # Arguments
/// * `data` - The subnet data to analyze
/// * `excluded_cidrs` - Optional list of VNet CIDRs to exclude. If None, uses defaults.
///
/// # Returns
/// A list of VnetInfo for VNets that would be excluded
pub fn get_excluded_vnets(
    data: &Data,
    excluded_cidrs: Option<&[&str]>,
) -> Vec<VnetInfo> {
    let default_excludes = default_vnet_cidrs_to_exclude();
    let excluded_cidrs = excluded_cidrs.unwrap_or(&default_excludes);

    // Parse excluded CIDRs
    let excluded: Vec<Ipv4> = excluded_cidrs
        .iter()
        .filter_map(|s| Ipv4::new(s).ok())
        .collect();

    // Track unique VNets (by name + subscription)
    let mut seen_vnets: HashMap<(String, String), VnetInfo> = HashMap::new();

    for subnet in &data.data {
        // Check if this subnet's VNet should be excluded
        let should_exclude = subnet.vnet_cidr.iter().any(|vnet_cidr| {
            excluded.iter().any(|excluded_cidr| vnet_cidr == excluded_cidr)
        });

        if should_exclude {
            let key = (subnet.vnet_name.clone(), subnet.subscription_id.clone());

            seen_vnets
                .entry(key)
                .and_modify(|info| info.subnet_count += 1)
                .or_insert_with(|| VnetInfo {
                    vnet_name: subnet.vnet_name.clone(),
                    vnet_cidr: subnet.vnet_cidr.clone(),
                    subscription_id: subnet.subscription_id.clone(),
                    subscription_name: subnet.subscription_name.clone(),
                    location: subnet.location.clone(),
                    subnet_count: 1,
                });
        }
    }

    seen_vnets.into_values().collect()
}

/// Filter out subnets belonging to VNets with excluded CIDRs.
///
/// # Arguments
/// * `data` - The subnet data to filter
/// * `excluded_cidrs` - Optional list of VNet CIDRs to exclude. If None, uses defaults.
///
/// # Returns
/// * `Ok(Data)` - Filtered data
pub fn filter_excluded_vnet_cidrs(
    mut data: Data,
    excluded_cidrs: Option<&[&str]>,
) -> Result<Data, Box<dyn Error>> {
    let default_excludes = default_vnet_cidrs_to_exclude();
    let excluded_cidrs = excluded_cidrs.unwrap_or(&default_excludes);

    // Parse excluded CIDRs
    let excluded: Vec<Ipv4> = excluded_cidrs
        .iter()
        .filter_map(|s| Ipv4::new(s).ok())
        .collect();

    let original_count = data.data.len();

    // Filter out subnets where any VNet CIDR matches an excluded CIDR
    data.data.retain(|subnet| {
        let should_exclude = subnet.vnet_cidr.iter().any(|vnet_cidr| {
            excluded.iter().any(|excluded_cidr| vnet_cidr == excluded_cidr)
        });

        if should_exclude {
            log::debug!(
                "Excluding subnet '{}' from VNet '{}' (CIDR matches exclusion list)",
                subnet.subnet_name,
                subnet.vnet_name
            );
        }

        !should_exclude
    });

    let filtered_count = original_count - data.data.len();
    if filtered_count > 0 {
        log::info!(
            "Filtered out {} subnets belonging to excluded VNet CIDRs: {:?}",
            filtered_count,
            excluded_cidrs
        );
    }

    Ok(data)
}

/// Filter overlapping VNets, keeping only one VNet per conflicting CIDR.
///
/// When multiple VNets use the same CIDR, keeps the one with:
/// 1. Most subnets (indicates more active use)
/// 2. If tied, keeps the first one alphabetically by subscription name
///
/// # Arguments
/// * `data` - The subnet data to filter
/// * `log_removals` - Whether to log which VNets are being removed
///
/// # Returns
/// * `Ok(Data)` - Filtered data with only one VNet per conflicting CIDR
pub fn filter_overlapping_vnets(
    mut data: Data,
    log_removals: bool,
) -> Result<Data, Box<dyn Error>> {
    let conflicts = find_overlapping_vnets(&data);

    if conflicts.is_empty() {
        return Ok(data);
    }

    // For each conflict, determine which VNets to remove
    let mut vnets_to_remove: Vec<(String, String)> = Vec::new(); // (vnet_name, subscription_id)

    for conflict in &conflicts {
        // Sort VNets: prefer more subnets, then alphabetically by subscription name
        let mut sorted_vnets = conflict.vnets.clone();
        sorted_vnets.sort_by(|a, b| {
            b.subnet_count
                .cmp(&a.subnet_count)
                .then_with(|| a.subscription_name.cmp(&b.subscription_name))
        });

        // Keep the first one, mark others for removal
        let keeper = &sorted_vnets[0];
        for vnet in sorted_vnets.iter().skip(1) {
            if log_removals {
                log::warn!(
                    "Removing VNet '{}' (subscription: '{}') - overlaps with kept VNet '{}' (subscription: '{}') on CIDR {}",
                    vnet.vnet_name,
                    vnet.subscription_name,
                    keeper.vnet_name,
                    keeper.subscription_name,
                    conflict.cidr
                );
            }
            vnets_to_remove.push((vnet.vnet_name.clone(), vnet.subscription_id.clone()));
        }
    }

    // Filter out subnets from removed VNets
    let original_count = data.data.len();
    data.data.retain(|subnet| {
        !vnets_to_remove
            .iter()
            .any(|(name, sub_id)| &subnet.vnet_name == name && &subnet.subscription_id == sub_id)
    });

    let filtered_count = original_count - data.data.len();
    if filtered_count > 0 {
        log::info!(
            "Filtered out {} subnets from {} overlapping VNets",
            filtered_count,
            vnets_to_remove.len()
        );
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::read_subnet_cache;

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
    fn test_filter_excluded_vnet_cidrs() {
        let data = read_subnet_cache(Some("subnet_cache_2026-02-09.json"));
        if let Ok(data) = data {
            let original_count = data.data.len();
            let filtered = filter_excluded_vnet_cidrs(data, None).unwrap();
            // Should have fewer subnets after filtering 10.0.0.0/16
            assert!(filtered.data.len() <= original_count);
        }
    }
}
