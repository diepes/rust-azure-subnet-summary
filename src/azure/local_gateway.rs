//! Azure Resource Graph query for Local Network Gateway data.
//!
//! Fetches site-to-site VPN connections and resolves which VNet each
//! Local Network Gateway (on-premises CIDR block) is associated with.

use super::cli;
use crate::config;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// KQL query: joins Connections → Local Network Gateways → VNet Gateways.
///
/// Returns one row per (gateway VNet, Local Network Gateway) pairing so that
/// callers can aggregate multiple LNGs per VNet.
///
/// Only IPsec (site-to-site VPN) connections are included — those are the only
/// connection type that has a `localNetworkGateway2` reference.
const LOCAL_GATEWAY_QUERY: &str = r#"resources
    | where type == "microsoft.network/connections"
    | where isnotnull(properties.localNetworkGateway2)
    | project
            vnet_gateway_id = tolower(tostring(properties.virtualNetworkGateway1.id))
           ,local_gateway_id = tolower(tostring(properties.localNetworkGateway2.id))
    | join kind=inner (
        resources
            | where type == "microsoft.network/localnetworkgateways"
            | project
                    local_gateway_id = tolower(id)
                   ,local_gw_name = name
                   ,address_prefixes = properties.localNetworkAddressSpace.addressPrefixes
                   ,gateway_ip = tostring(properties.gatewayIpAddress)
                   ,gateway_ips = properties.gatewayIpAddresses
                   ,bgp_asn = tostring(properties.bgpSettings.asn)
                   ,bgp_peer_ip = tostring(properties.bgpSettings.bgpPeeringAddress)
        ) on local_gateway_id
    | join kind=inner (
        resources
            | where type == "microsoft.network/virtualnetworkgateways"
            | mv-expand ip_cfg = properties.ipConfigurations
            | extend subnet_id = tolower(tostring(ip_cfg.properties.subnet.id))
            | where subnet_id contains "/subnets/gatewaysubnet"
            | project
                    vnet_gateway_id = tolower(id)
                   ,vng_name = name
                   ,vng_bgp_asn = tostring(properties.bgpSettings.asn)
                   ,vnet_name = tostring(split(subnet_id, "/")[8])
            | summarize vnet_name = any(vnet_name), vng_name = any(vng_name), vng_bgp_asn = any(vng_bgp_asn) by vnet_gateway_id
        ) on vnet_gateway_id
    | project vnet_name, vng_name, vng_bgp_asn, local_gw_name, address_prefixes, gateway_ip, gateway_ips, bgp_asn, bgp_peer_ip
    | sort by vnet_name asc, local_gw_name asc"#;

/// Deserialize a JSON value that may be either an array of strings or `null`
/// into a `Vec<String>`, treating `null` as an empty vector.
fn null_as_empty_vec<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<String>>::deserialize(de).map(|v| v.unwrap_or_default())
}

/// One row from the local gateway query: the on-premises connection for a gateway VNet.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct LocalGatewayRow {
    /// Name of the Azure VNet that hosts the VPN gateway.
    pub vnet_name: String,
    /// Name of the Azure VPN Gateway resource (e.g. "sandbox-vpngw-wus").
    #[serde(default)]
    pub vng_name: String,
    /// BGP ASN configured on the Azure VPN Gateway (empty string if BGP disabled).
    #[serde(default)]
    pub vng_bgp_asn: String,
    /// Name of the Local Network Gateway (represents the on-premises site).
    pub local_gw_name: String,
    /// On-premises address prefixes (CIDRs) configured on this Local Network Gateway.
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub address_prefixes: Vec<String>,
    /// Public IP of the on-premises VPN device (`properties.gatewayIpAddress`, single IP).
    #[serde(default)]
    pub gateway_ip: String,
    /// Public IPs for active-active / BGP configs (`properties.gatewayIpAddresses` array).
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub gateway_ips: Vec<String>,
    /// On-premises BGP Autonomous System Number (empty string if BGP disabled).
    #[serde(default)]
    pub bgp_asn: String,
    /// On-premises BGP peering IP address (empty string if BGP disabled).
    #[serde(default)]
    pub bgp_peer_ip: String,
}

/// Response wrapper for the local gateway query.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct LocalGatewayData {
    pub data: Vec<LocalGatewayRow>,
    pub skip_token: Option<String>,
    pub total_records: Option<u32>,
    pub count: i32,
}

/// Execute the Azure Resource Graph local gateway query with automatic pagination.
pub fn run_local_gateway_graph() -> Result<LocalGatewayData, Box<dyn Error>> {
    let mut data: LocalGatewayData = Default::default();
    let mut skip_token_param = String::new();
    let mut count_blocks_returned = 0;

    while skip_token_param != "--skip-token null" {
        let cmd = format!(
            "az graph query --first 50 {skip_token_param} -q '{LOCAL_GATEWAY_QUERY}' --output json"
        );
        let output = cli::run(&cmd)?;

        let mut json_block_deserializer = serde_json::Deserializer::from_str(&output);
        let json_parsed: LocalGatewayData =
            serde_path_to_error::deserialize(&mut json_block_deserializer).map_err(|e| {
                log::error!("OUTPUT START:\n\n{}\n\nOUTPUT END\n", output);
                format!(
                    "Error parsing local gateway JSON block {}: path={} error={}",
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
            return Err("Local gateway skip token not unique - possible infinite loop".into());
        }
        skip_token_param = skip_token_new;

        let count = json_parsed.count;
        data.data.extend(json_parsed.data);
        data.count += count;

        if let Some(total) = json_parsed.total_records {
            data.total_records = Some(total);
        }

        log::info!(
            "got local-gw block#{count_blocks_returned:2} record_count=+{count:3} => {total:3} skip_token='{skip_token_param}'",
            total = data.count,
        );

        std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 5));
        count_blocks_returned += 1;
    }

    Ok(data)
}
