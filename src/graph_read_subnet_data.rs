use crate::cmd;
use crate::config;
use crate::struct_subnet::Subnet;
use chrono;
//use chrono::TimeZone;
/// Runs az cli graph to read subnets
use serde::{Deserialize, Serialize};

/// fn read_subnet_cache()
/// Reads from cache if exists else call run_az_cli_graph() to get data
pub fn read_subnet_cache(cache_file: Option<&str>) -> Result<Data, Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Pacific::Auckland);
    // if cache_file is provided, use it, else create a default cache file name
    let cache_file = match cache_file {
        Some(file) => {
            // Panic if the provided cache file does not exist
            if !std::path::Path::new(file).exists() {
                panic!("Cache file does not exist: {}", file);
            }
            log::info!("Using provided cache file: {}", file);
            file.to_string()
        }
        None => format!("subnet_cache_{}.json", now.format("%Y-%m-%d").to_string()),
    };
    let data_from_cache_or_cli: Data = match std::fs::read_to_string(&cache_file) {
        Ok(json) => {
            log::info!("Reading from cache file: {}", cache_file);
            serde_json::from_str(&json).expect("Error parsing json")
        }
        Err(_) => {
            log::warn!("Cache file not found: {}", cache_file);
            let data = run_az_cli_graph()?;
            log::info!("parse json data received from azure cli");
            let json = serde_json::to_string(&data).expect("Error serializing json");
            log::warn!("Write data to Cache file: {}", cache_file);
            std::fs::write(&cache_file, json)
                .expect(format!("Error writing cache file {cache_file}").as_str());
            data
        }
    };
    Ok(data_from_cache_or_cli)
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Data {
    pub data: Vec<Subnet>,
    pub skip_token: Option<String>,
    pub total_records: Option<u32>,
    pub count: i32,
}

pub fn run_az_cli_graph() -> Result<Data, Box<dyn std::error::Error>> {
    // let output = cmd::run_az_cli_graph().expect("Error running az cli graph");
    let mut data: Data = Default::default();
    let mut skip_token_param: String = "".to_string();
    let mut count_blocks_returned = 0;
    let mut src_index: usize = 0; // save index count of record returned from 0..
    while skip_token_param != "--skip-token null".to_string() {
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
        | sort by vnet_name asc' --output json"
        ))
        .expect("Error running az graph query");

        let mut json_block_deserializer = serde_json::Deserializer::from_str(&output);
        let json_block_results: Result<Data, serde_path_to_error::Error<serde_json::Error>> =
            serde_path_to_error::deserialize(&mut json_block_deserializer);
        // Unwrap the block of data from the json
        let json_parsed = match json_block_results {
            Ok(s) => s,
            Err(e) => {
                let json_path = e.path().to_string();
                log::error!("OUTPUT START:\n\n{}\n\nOUTPUT END\n", output); //&output[output.len() - 400..]);
                panic!(
                    "Error parsing json block {}: ErrPath:{:?} e:{:?}",
                    count_blocks_returned, json_path, e
                );
            }
        };

        let skip_token_new = json_parsed.skip_token.unwrap_or("null".to_string());
        let skip_token_new = format!("--skip-token {skip_token_new}",);
        // assert that skip_token's are unique
        assert_ne!(
            skip_token_new, skip_token_param,
            "skip_token_new == skip_token_param not unique ???"
        );
        skip_token_param = skip_token_new;

        data.data
            .extend(json_parsed.data.into_iter().enumerate().map(|(i, mut s)| {
                s.src_index = i; //src_index;
                s.block_id = count_blocks_returned;
                src_index += 1;
                s
            }));
        let count = json_parsed.count;
        data.count = data.count + json_parsed.count;
        if let Some(block_records) = json_parsed.total_records {
            data.total_records = Some(block_records);
        }
        log::info!(
            "got block#{count_blocks_returned:2} record_count=+{obj_count:3} => {dc:3} skip_token_param='{skip_token_snippit}'",
            dc = data.count,
            obj_count = count,
            // skip_token_snippit = format!("{}...{}", &skip_token_param[0..16], &skip_token_param[skip_token_param.len() - 3..]),
            skip_token_snippit = format!("{}", &skip_token_param),
        );
        // pause to see output
        std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 5));
        // Next block
        count_blocks_returned += 1;
    } // end of while loop
      // let mut json_vec = cmd::string_to_json_vec_map(&output)?;
    log::info!(
        "Got data #{} == {} records from az graph query, src_index={src_index}",
        data.count,
        data.data.len()
    );
    assert_eq!(src_index, data.data.len(), "src_index != data.data.len()");
    // pause to see output
    log::info!("sleep 15s ...");
    std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 15));
    Ok(data)
}

// TESTS to read data from test/test_data/az_vm_output_01.json
#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_read_subnet_cache() {
        // Test reading from cache file form path in tests folder
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_01.json"))
            .expect("Error reading subnet cache");
        assert!(!data.data.is_empty(), "Data should not be empty");
        assert_eq!(
            data.data[0].vnet_name, "z-env-shared_services-vnet-AbCdEf",
            "Wrong vnet from test sample."
        );
        assert!(data.total_records.is_some(), "Total records should be set");
        assert!(data.count > 0, "Count should be greater than 0");
        log::info!("Data read from cache: {:?}", data);
    }
    #[tokio::test]
    async fn test_read_subnet_cache_04() {
        // Test reading from cache file form path in tests folder
        let test_cache = "src/tests/test_data/subnet_test_cache_04.json";
        let data = read_subnet_cache(Some(test_cache)).expect("Error reading subnet cache");
        assert!(!data.data.is_empty(), "Data should not be empty");
        assert_eq!(
            data.data.len(),
            180,
            "Expected 180 subnets in test sample {test_cache}"
        );
        assert_eq!(
            data.data[0].vnet_name, "Docker_vSEC",
            "Wrong vnet from test sample."
        );
        assert!(data.total_records.is_some(), "Total records should be set");
        assert!(data.count > 0, "Count should be greater than 0");
        log::info!("Data read from cache: {:?}", data);
    }
}
