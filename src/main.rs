//! Azure Subnet Summary - Main entry point
//!
//! This tool queries Azure Resource Graph to get subnet information,
//! identifies gaps in IP address allocation, and outputs a CSV summary.

use azure_subnet_summary::{
    check_for_duplicate_subnets, get_sorted_subnets,
    processing::{de_duplicate_subnets, get_vnets, print_vnets},
    output::subnet_print,
};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    log4rs::init_file("log4rs.yml", Default::default()).expect("Error initializing log4rs");
    dotenv::dotenv().ok();

    log::info!("#Start main()");

    // Fetch and process subnet data
    let data = get_sorted_subnets(None)?;
    let data = de_duplicate_subnets(data, None)?;
    check_for_duplicate_subnets(&data)?;

    // Output subnet summary
    const DEFAULT_CIDR_MASK: u8 = 16;
    subnet_print(&data, DEFAULT_CIDR_MASK)?;

    // Output VNet summary
    let vnets = get_vnets(&data)?;
    print_vnets(&vnets)?;

    Ok(())
}
