//! Azure Subnet Summary - Main entry point
//!
//! This tool queries Azure Resource Graph to get subnet information,
//! identifies gaps in IP address allocation, and outputs a CSV summary.

use azure_subnet_summary::{
    azure::{read_peering_cache_with_status, PeeringCacheResult},
    check_for_duplicate_subnets, get_sorted_subnets_with_status,
    output::{subnet_print, write_duplicates_md, write_peering_diagram},
    processing::{
        de_duplicate_subnets, filter_overlapping_vnets,
        find_overlapping_vnets, get_vnets, log_overlapping_vnets, print_vnets,
    },
};
use clap::Parser;
use std::error::Error;

/// Azure Subnet Summary - maps IP allocation and identifies free gaps.
#[derive(Parser, Debug)]
#[command(name = "azure-subnet-summary", about = "Summarise Azure subnets and IP gaps")]
struct Args {
    /// Minimum gap-block mask (smaller number = bigger blocks).
    /// /4 means gaps up to a /4 (covering 1/16th of IPv4 space) are emitted
    /// as a single row instead of many /16 rows.
    #[arg(long, default_value_t = 4, value_name = "N")]
    gap_mask: u8,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

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

    // Filter overlapping VNets (production subscription wins; marks losers with excluded_by)
    // This must happen before gap-finding, which assumes subnets are non-overlapping
    let data = filter_overlapping_vnets(data, true)?;

    let data = de_duplicate_subnets(data, None)?;
    check_for_duplicate_subnets(&data)?;

    // Output subnet summary
    let csv_file = subnet_print(&data, args.gap_mask)?;

    // Output duplicate VNet report
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dup_file = format!("subnets-{date_str}-duplicates.md");
    write_duplicates_md(&data, &dup_file)?;
    log::info!("Duplicates report written to '{dup_file}'");

    // Output peering diagram
    let PeeringCacheResult { data: peering_data, from_cache: peering_from_cache, cache_file: peering_cache_file } =
        read_peering_cache_with_status(None)?;
    let peering_source = if peering_from_cache {
        format!("existing cache '{peering_cache_file}'")
    } else {
        format!("Azure (new cache written to '{peering_cache_file}')")
    };
    let peering_file = format!("subnets-{date_str}-peering.md");
    write_peering_diagram(&peering_data.data, &data, &peering_file)?;
    log::info!("Peering diagram written to '{peering_file}' from {peering_source}");

    // Output VNet summary
    let vnets = get_vnets(&data)?;
    print_vnets(&vnets, None)?;

    // Final summary
    log::info!("Complete: Generated '{}' from {}", csv_file, cache_source);

    Ok(())
}
