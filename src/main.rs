use azure_subnet_summary::check_for_duplicate_subnets;
use azure_subnet_summary::de_duplicate_subnets2;
use azure_subnet_summary::get_sorted_subnets;
use azure_subnet_summary::print_subnets::print_subnets;
use azure_subnet_summary::struct_vnet::get_vnets;
use azure_subnet_summary::struct_vnet::print_vnets;
use log4rs;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Do as little as possible in main.rs as it can't contain any tests
    log4rs::init_file("log4rs.yml", Default::default()).expect("Error initializing log4rs");
    dotenv::dotenv().ok();
    //
    log::info!("#Start main()");

    let data = get_sorted_subnets(None).expect("Error reading subnets form cache or az cli graph");
    let data = de_duplicate_subnets2(data, None).expect("Error deduplicating subnets");
    check_for_duplicate_subnets(&data).expect("Error validating subnets");

    const DEFAULT_CIDR_MASK: u8 = 26; // /28 = 11 ips for hosts in Azure. (16-5)
    print_subnets(&data, DEFAULT_CIDR_MASK).await?;
    let vnets = get_vnets(&data).expect("Error getting vnets");
    print_vnets(&vnets).await?;

    Ok(())
}
