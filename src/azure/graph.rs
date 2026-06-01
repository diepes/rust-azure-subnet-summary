//! Azure Resource Graph query execution.
//!
//! Handles querying Azure Resource Graph for subnet information.

use super::{cli, paginate::paginate};
use crate::config;
use crate::models::Subnet;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;

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
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
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
    let sleep = Duration::from_millis(config::SLEEP_MSEC * 5);
    let rows = paginate(SUBNET_QUERY, sleep, cli::run)?;

    let data: Vec<Subnet> = serde_json::from_value(serde_json::Value::Array(rows))
        .map_err(|e| format!("Error parsing subnet rows: {e}"))?;

    let count = data.len() as i32;
    let total_records = Some(data.len() as u32);

    log::info!(
        "Got data #{count} == {} records from az graph query",
        data.len()
    );

    log::info!("sleep 15s ...");
    std::thread::sleep(Duration::from_millis(config::SLEEP_MSEC * 15));

    Ok(Data {
        data,
        skip_token: None,
        total_records,
        count,
    })
}
