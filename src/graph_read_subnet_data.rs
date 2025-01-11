use crate::{cmd, ipv4::Ipv4};
/// Runs az cli graph to read subnets
use serde::{Deserialize, Serialize};

/// fn read_subnet_cache()
/// Reads from cache if exists else call run_az_cli_graph() to get data
pub fn read_subnet_cache() -> Result<Data, Box<dyn std::error::Error>> {
    let cache_file = "subnet_cache.json";
    let data_from_cache_or_cli: Data = match std::fs::read_to_string(cache_file) {
        Ok(json) => {
            log::info!("Reading from cache file: {}", cache_file);
            serde_json::from_str(&json).expect("Error parsing json")
        }
        Err(_) => {
            log::warn!("Cache file not found: {}", cache_file);
            let data = run_az_cli_graph()?;
            log::info!("parse json data received from azure cli");
            let json = serde_json::to_string(&data).expect("Error serializing json");
            log::warn!("Write cata to Cache file: {}", cache_file);
            std::fs::write(cache_file, json).expect("Error writing cache file");
            data
        }
    };
    Ok(data_from_cache_or_cli)
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Data {
    pub data: Vec<Subnet>,
    skip_token: Option<String>,
    pub total_records: Option<u32>,
    pub count: i32,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Subnet {
    pub vnet_name: String,
    pub vnet_cidr: Vec<Ipv4>,
    pub subnet_name: String,
    pub subnet_cidr: Option<Ipv4>,
    pub nsg: Option<String>,
    pub location: String,
    pub dns_servers: Option<Vec<String>>,
    pub subscription_id: String,
    pub subscription_name: String,
    // Fill value to gap if we create new subnet
    pub gap: Option<String>,
    // Serde field to ignore and set default value
    #[serde(skip)]
    pub src_index: i32,
}

pub fn run_az_cli_graph() -> Result<Data, Box<dyn std::error::Error>> {
    // let output = cmd::run_az_cli_graph().expect("Error running az cli graph");
    let mut data: Data = Default::default();
    let mut skip_token_param: String = "".to_string();
    let mut count_blocks_returned = 0;
    let mut src_index = 0; // save index count of record returned from 0..
    while skip_token_param != "--skip-token null".to_string() {
        count_blocks_returned += 1;
        let output = cmd::run(&format!(
            "az graph query --first 50 {skip_token_param} -q 'resources 
        | where type == \"microsoft.network/virtualnetworks\"
        | mv-expand properties.subnets 
        | project subscription_id=subscriptionId
                ,vnet_name=name
                ,vnet_cidr=properties.addressSpace.addressPrefixes
                ,subnet_name=properties_subnets.name
                ,subnet_cidr=properties_subnets.properties.addressPrefix
                ,nsg=properties_subnets.properties.networkSecurityGroup.id
                ,location=location
                ,dns_servers=properties.dhcpOptions.dnsServers
        | join kind=leftouter (
            resourcecontainers
                | where type == \"microsoft.resources/subscriptions\"
                | project subscription_id=subscriptionId, subscription_name=name
            ) on subscription_id
| project subscription_id, subscription_name, vnet_name, vnet_cidr, subnet_name, subnet_cidr, nsg, location, dns_servers
| sort by vnet_name asc
        | sort by vnet_name asc' --output json"
        ))
        .expect("Error running az graph query");

        let mut json_block_deserializer = serde_json::Deserializer::from_str(&output);
        let json_block_results: Result<Data, serde_path_to_error::Error<serde_json::Error>> =
            serde_path_to_error::deserialize(&mut json_block_deserializer);
        let block = match json_block_results {
            Ok(d) => d,
            Err(e) => {
                let json_path = e.path().to_string();
                log::error!("OUTPUT START:\n\n{}\n\nOUTPUT END\n", output); //&output[output.len() - 400..]);
                panic!(
                    "Error parsing json block {}: ErrPath:{:?} e:{:?}",
                    count_blocks_returned, json_path, e
                );
            }
        };
        // let block: Data = serde_json::from_str(&output).expect(&format!(
        //     "Error parsing json block {}: \nOUTPUT: \n...\n{}\n",
        //     count_blocks_returned,
        //     &output[output.len() - 400..]
        // ));
        // retrieve skip_token from block
        let skip_token_new = block.skip_token.unwrap_or("null".to_string());
        skip_token_param = format!("--skip-token {skip_token_new}",);
        let count = block.count;
        log::info!(
            "got block {block:3} record_count = {dc:3} + {obj_count:3} skip_token_param='{skip_token_snippit}'",
            block = count_blocks_returned,
            dc = data.count,
            obj_count = count,
            skip_token_snippit = format!("{}...{}", &skip_token_param[0..16], &skip_token_param[skip_token_param.len() - 3..]),
        );
        data.data.extend(block.data.into_iter().map(|mut s| {
            s.src_index = src_index as i32;
            src_index += 1;
            s
        }));
        data.count = data.count + count;
        if let Some(total_records) = block.total_records {
            data.total_records = Some(total_records);
        }
    }
    // let mut json_vec = cmd::string_to_json_vec_map(&output)?;
    log::info!(
        "Got data #{} == {} records from az graph query, index={src_index}",
        data.count,
        data.data.len()
    );
    Ok(data)
}
