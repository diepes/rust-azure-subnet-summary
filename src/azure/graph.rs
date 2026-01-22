//! Azure Resource Graph query execution.
//!
//! Handles querying Azure Resource Graph for subnet information.

use super::cli;
use crate::config;
use crate::models::Subnet;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// Azure Graph query for fetching subnet data.
const SUBNET_QUERY: &str = r#"resources 
        | where type == "microsoft.network/virtualnetworks"
        | mv-expand properties.subnets 
        | project subscription_id=subscriptionId
                ,vnet_name=name
                ,vnet_cidr=properties.addressSpace.addressPrefixes
                ,subnet_name=properties_subnets.name
                ,subnet_cidr=properties_subnets.properties.addressPrefix
                ,nsg=properties_subnets.properties.networkSecurityGroup.id
                ,location=location
                ,dns_servers=properties.dhcpOptions.dnsServers
                ,ip_configurations_count=array_length(properties_subnets.properties.ipConfigurations)
        | join kind=leftouter (
            resourcecontainers
                | where type == "microsoft.resources/subscriptions"
                | project subscription_id=subscriptionId, subscription_name=name
            ) on subscription_id
        | project subscription_id, subscription_name, vnet_name, vnet_cidr, subnet_name, subnet_cidr, nsg, location, dns_servers, ip_configurations_count
        | sort by vnet_name asc"#;

/// Response data from Azure Graph query.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Data {
    /// List of subnets returned.
    pub data: Vec<Subnet>,
    /// Token for pagination (if more results available).
    pub skip_token: Option<String>,
    /// Total number of records matching the query.
    pub total_records: Option<u32>,
    /// Count of records in this response.
    pub count: i32,
}

/// Execute Azure Resource Graph query to fetch all subnets.
///
/// Handles pagination automatically using skip tokens.
///
/// # Returns
/// * `Ok(Data)` - All subnet data from Azure
/// * `Err` - If the query fails
pub fn run_az_cli_graph() -> Result<Data, Box<dyn Error>> {
    let mut data: Data = Default::default();
    let mut skip_token_param: String = String::new();
    let mut count_blocks_returned = 0;
    let mut src_index: usize = 0;

    while skip_token_param != "--skip-token null" {
        let cmd = format!(
            "az graph query --first 50 {skip_token_param} -q '{SUBNET_QUERY}' --output json"
        );
        let output = cli::run(&cmd)?;

        let mut json_block_deserializer = serde_json::Deserializer::from_str(&output);
        let json_parsed: Data = serde_path_to_error::deserialize(&mut json_block_deserializer)
            .map_err(|e| {
                log::error!("OUTPUT START:\n\n{}\n\nOUTPUT END\n", output);
                format!(
                    "Error parsing JSON block {}: path={} error={}",
                    count_blocks_returned,
                    e.path(),
                    e
                )
            })?;

        let skip_token_new = json_parsed
            .skip_token
            .clone()
            .unwrap_or_else(|| "null".to_string());
        let skip_token_new = format!("--skip-token {skip_token_new}");

        if skip_token_new == skip_token_param {
            return Err("Skip token not unique - possible infinite loop".into());
        }
        skip_token_param = skip_token_new;

        data.data
            .extend(json_parsed.data.into_iter().enumerate().map(|(i, mut s)| {
                s.src_index = i;
                s.block_id = count_blocks_returned;
                src_index += 1;
                s
            }));

        let count = json_parsed.count;
        data.count += json_parsed.count;

        if let Some(block_records) = json_parsed.total_records {
            data.total_records = Some(block_records);
        }

        log::info!(
            "got block#{count_blocks_returned:2} record_count=+{count:3} => {total:3} skip_token='{skip_token_param}'",
            total = data.count,
        );

        // Rate limiting pause
        std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 5));
        count_blocks_returned += 1;
    }

    log::info!(
        "Got data #{} == {} records from az graph query, src_index={src_index}",
        data.count,
        data.data.len()
    );

    if src_index != data.data.len() {
        return Err(format!(
            "Index mismatch: src_index={} != data.len()={}",
            src_index,
            data.data.len()
        )
        .into());
    }

    log::info!("sleep 15s ...");
    std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 15));

    Ok(data)
}
