use azure_subnet_summary::check_for_duplicate_subnets;
use azure_subnet_summary::de_duplicate_subnets;
use azure_subnet_summary::get_sorted_subnets;
use azure_subnet_summary::get_vnets;
use azure_subnet_summary::print_subnets::print_subnets;
use log4rs;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Do as little as possible in main.rs as it can't contain any tests
    log4rs::init_file("log4rs.yml", Default::default()).expect("Error initializing log4rs");
    dotenv::dotenv().ok();
    //
    log::info!("#Start main()");

    let data = get_sorted_subnets().expect("Error reading subnets form cache or az cli graph");
    let _vnets = get_vnets(&data).expect("Error getting vnets");
    let data = de_duplicate_subnets(data, None).expect("Error deduplicating subnets");
    check_for_duplicate_subnets(&data).expect("Error validating subnets");

    print_subnets(data).await?;

    Ok(())
}
