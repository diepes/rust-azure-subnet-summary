//! Azure Subnet Summary Library
//!
//! This library provides functionality to query and analyze Azure virtual network subnets.
//!
//! # Modules
//! - [`models`] - Core data models (Ipv4, Subnet, Vnet)
//! - [`azure`] - Azure CLI and Graph API interaction
//! - [`processing`] - Business logic for subnet processing
//! - [`output`] - Output formatting (CSV, terminal)
//!
//! # Example
//! ```no_run
//! use azure_subnet_summary::{get_sorted_subnets, check_for_duplicate_subnets};
//!
//! let data = get_sorted_subnets(None).expect("Failed to get subnets");
//! check_for_duplicate_subnets(&data).expect("Found duplicates");
//! ```

// New modular structure
pub mod azure;
pub mod models;
pub mod output;
pub mod processing;

// Legacy modules (for backwards compatibility during migration)
mod cmd;
mod config;
mod de_duplicate_subnets;
mod graph_read_subnet_data;
mod ipv4;
pub mod struct_vnet;
pub mod subnet_add_row;
pub mod subnet_print;
mod subnet_struct;

use std::collections::HashSet;

// Re-export commonly used types from new modules
pub use azure::Data;
pub use models::{Ipv4, Subnet, Vnet, VnetList};
pub use output::subnet_print as print_subnets;
pub use processing::{
    de_duplicate_subnets, filter_excluded_vnet_cidrs, filter_overlapping_vnets,
    find_overlapping_vnets, get_excluded_vnets, get_vnets, log_overlapping_vnets, print_vnets,
    SubnetPrintRow, VnetInfo,
};

/// Get sorted subnet data from cache or Azure.
///
/// # Arguments
/// * `cache_file` - Optional path to cache file. If None, uses default naming.
///
/// # Returns
/// * `Ok(Data)` - Sorted subnet data
/// * `Err` - If reading or parsing fails
pub fn get_sorted_subnets(
    cache_file: Option<&str>,
) -> Result<azure::Data, Box<dyn std::error::Error>> {
    let mut data = azure::read_subnet_cache(cache_file)?;
    data.data.sort_by_key(|s| s.subnet_cidr);
    Ok(data)
}

/// Check for duplicate subnets in the data.
///
/// # Arguments
/// * `data` - The subnet data to check
///
/// # Returns
/// * `Ok(())` - No duplicates found
/// * `Err` - If a duplicate is found
#[must_use = "This function returns a Result that should be checked"]
pub fn check_for_duplicate_subnets(data: &azure::Data) -> Result<(), Box<dyn std::error::Error>> {
    let mut seen = HashSet::new();

    for sub in data.data.iter() {
        if !seen.insert((sub.subnet_cidr, sub.subscription_id.clone())) {
            return Err(format!("Duplicate found: {sub:?}").into());
        }
    }
    Ok(())
}

// Legacy re-exports for backwards compatibility
pub use de_duplicate_subnets::de_duplicate_subnets2;
pub use struct_vnet::get_vnets as get_vnets_legacy;

/// Legacy version of get_sorted_subnets that returns the old Data type.
/// Use `get_sorted_subnets` for new code.
#[doc(hidden)]
pub fn get_sorted_subnets_legacy(
    cache_file: Option<&str>,
) -> Result<graph_read_subnet_data::Data, Box<dyn std::error::Error>> {
    let mut data = graph_read_subnet_data::read_subnet_cache(cache_file)?;
    data.data.sort_by_key(|s| s.subnet_cidr);
    Ok(data)
}
