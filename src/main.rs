//! Azure Subnet Summary - Main entry point
//!
//! This tool queries Azure Resource Graph to get subnet information,
//! identifies gaps in IP address allocation, and outputs a CSV summary.

use azure_subnet_summary::{
    azure::{fetch_azure_data, FetchConfig},
    pipeline::{run, Args, GraphvizRenderer},
};
use clap::Parser;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Initialize logging — fall back to default console logger if config file is missing
    if log4rs::init_file("log4rs.yml", Default::default()).is_err() {
        let stdout = log4rs::append::console::ConsoleAppender::builder().build();
        let config = log4rs::Config::builder()
            .appender(log4rs::config::Appender::builder().build("stdout", Box::new(stdout)))
            .build(
                log4rs::config::Root::builder()
                    .appender("stdout")
                    .build(log::LevelFilter::Info),
            )?;
        log4rs::init_config(config)?;
    }
    dotenv::dotenv().ok();

    log::info!("#Start main()");

    let date_str = chrono::Utc::now()
        .with_timezone(&chrono_tz::Pacific::Auckland)
        .format("%Y-%m-%d")
        .to_string();
    let cache_dir = format!("report-{date_str}/cache");
    std::fs::create_dir_all(&cache_dir)?;

    let azure = fetch_azure_data(&FetchConfig {
        cache_dir: Some(cache_dir),
        ..FetchConfig::default()
    })?;
    run(azure, &args, &GraphvizRenderer)?;

    Ok(())
}
