//! Azure Subnet Summary - Main entry point
//!
//! This tool queries Azure Resource Graph to get subnet information,
//! identifies gaps in IP address allocation, and outputs a CSV summary.

use azure_subnet_summary::{
    check_for_duplicate_subnets, get_sorted_subnets_with_status,
    output::subnet_print,
    processing::{
        de_duplicate_subnets, filter_excluded_vnet_cidrs, find_overlapping_vnets,
        get_excluded_vnets, get_vnets, log_overlapping_vnets, print_vnets,
    },
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging - fall back to default console logger if config file is missing
    if log4rs::init_file("log4rs.yml", Default::default()).is_err() {
        let stdout = log4rs::append::console::ConsoleAppender::builder().build();
        let config = log4rs::Config::builder()
            .appender(log4rs::config::Appender::builder().build("stdout", Box::new(stdout)))
            .build(log4rs::config::Root::builder().appender("stdout").build(log::LevelFilter::Info))?;
        log4rs::init_config(config)?;
    }
    dotenv::dotenv().ok();

    log::info!("#Start main()");

    // Fetch and process subnet data (with cache status)
    let cache_result = get_sorted_subnets_with_status(None)?;
    let cache_source = if cache_result.from_cache {
        format!("existing cache '{}'", cache_result.cache_file)
    } else {
        format!("Azure (new cache written to '{}')", cache_result.cache_file)
    };
    let data = cache_result.data;

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
    let csv_file = subnet_print(&data, DEFAULT_CIDR_MASK)?;

    // Output VNet summary (including excluded VNets)
    let vnets = get_vnets(&data)?;
    print_vnets(&vnets, Some(&excluded_vnets))?;

    // Final summary
    log::info!("Complete: Generated '{}' from {}", csv_file, cache_source);

    Ok(())
}
