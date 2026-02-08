//! Azure Subnet Summary - Main entry point
//!
//! This tool queries Azure Resource Graph to get subnet information,
//! identifies gaps in IP address allocation, and outputs a CSV summary.

use azure_subnet_summary::{
    check_for_duplicate_subnets, get_sorted_subnets,
    output::subnet_print,
    processing::{
        de_duplicate_subnets, filter_excluded_vnet_cidrs, find_overlapping_vnets,
        get_excluded_vnets, get_vnets, log_overlapping_vnets, print_vnets,
    },
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    log4rs::init_file("log4rs.yml", Default::default()).expect("Error initializing log4rs");
    dotenv::dotenv().ok();

    log::info!("#Start main()");

    // Fetch and process subnet data
    let data = get_sorted_subnets(None)?;

    // Check for and log overlapping VNet CIDRs
    let conflicts = find_overlapping_vnets(&data);
    log_overlapping_vnets(&conflicts);

    // Get excluded VNets before filtering (for display purposes)
    let excluded_vnets = get_excluded_vnets(&data, None);

    // Filter out common "local-use" VNet CIDRs (e.g., 10.0.0.0/16)
    // that are duplicated across subscriptions and not globally routable
    let data = filter_excluded_vnet_cidrs(data, None)?;

    let data = de_duplicate_subnets(data, None)?;
    check_for_duplicate_subnets(&data)?;

    // Output subnet summary
    const DEFAULT_CIDR_MASK: u8 = 16;
    subnet_print(&data, DEFAULT_CIDR_MASK)?;

    // Output VNet summary (including excluded VNets)
    let vnets = get_vnets(&data)?;
    print_vnets(&vnets, Some(&excluded_vnets))?;

    Ok(())
}
