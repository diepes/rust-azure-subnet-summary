// src/de_duplicate_subnets.rs

use crate::graph_read_subnet_data;
use crate::ipv4::Ipv4;
use crate::struct_subnet::Subnet;
use colored::Colorize;
use std::error::Error;
use std::net::Ipv4Addr;
fn default_subnet_names_to_ignore() -> Vec<&'static str> {
    vec![
        "default",
        "jenkinsarm-snet",
        "pkrsn1ooslfxj77",
        "pkrsn8jufz9plf6",
        "pkrsnsnajtq3h3i",
        "pkrsnxocivqofa6",
        "twggmcmg",
        "restore-vm-subnet",
    ]
}
pub fn de_duplicate_subnets(
    mut data: graph_read_subnet_data::Data,
    subnet_names_to_ignore: Option<Vec<&str>>,
) -> Result<graph_read_subnet_data::Data, Box<dyn Error>> {
    // if subnet_names_to_ignore is None, use the default list
    let subnet_names_to_ignore = match subnet_names_to_ignore
    {
        Some(names) => names,
        None => default_subnet_names_to_ignore(),
    };
    let mut cidr: Ipv4 = Ipv4::new("11.2.0.0/32").unwrap();
    let mut prev_subnet = Subnet::default();
    let mut dup = (0, 9999);
    let mut i = 0;
    while i < data.data.len() {
        let s = &data.data[i];
        // Check for duplicate src_index and block_id
        if (s.src_index, s.block_id) == dup {
            panic!(" Duplicate src_index: {:?}", dup);
        }
        dup = (s.src_index, s.block_id);
        log::debug!(
            "index: {} subnet_name: {} subnet_cidr: {:?} prev_subnet_cidr {:?} cidr: {}",
            i,
            s.subnet_name,
            s.subnet_cidr,
            prev_subnet.subnet_cidr,
            cidr
        );
        if s.subnet_cidr.is_none() {
            log::info!(
                "SKIPPING {i} EMPTY: subnet_cidr: None. subnet_name: {} prev_cidr: {cidr}",
                s.subnet_name
            );
            std::thread::sleep(std::time::Duration::from_millis(
                crate::config::SLEEP_MSEC * 2,
            ));
            data.data.remove(i);
            continue;
        };
        if s.subnet_cidr
            == Some(Ipv4 {
                addr: Ipv4Addr::new(10, 0, 0, 0),
                mask: 24,
            })
            && subnet_names_to_ignore.contains(&s.subnet_name.as_str())
        {
            log::warn!(
                "SKIPPING {i} default: subnet_cidr: None. subnet_name: {} prev_cidr: {cidr}",
                s.subnet_name
            );
            std::thread::sleep(std::time::Duration::from_millis(
                crate::config::SLEEP_MSEC * 1,
            ));
            data.data.remove(i);
            continue;
        };
        //TODO: should be normal filter by name
        if s.subnet_cidr
            == Some(Ipv4 {
                addr: Ipv4Addr::new(10, 1, 1, 0),
                mask: 24,
            })
            && ["restore-vm-subnet"].contains(&s.subnet_name.as_str())
        {
            log::info!(
                "SKIPPING {i} restore-vm-subnet: subnet_cidr: None. subnet_name: {} prev_cidr: {cidr}",
                s.subnet_name
            );
            data.data.remove(i);
            continue;
        };
        if cidr == s.subnet_cidr.unwrap() {
            let msg_duplicate = format!(
                "Duplicate[A] cidr:{cidr} Sub['{subnet_name}'],\n    Name[{prev_index}]{prev_sub_name} Sub['{prev_sub}'] cidr:{cidr_prev}\n    Name[{index}]{subnet_name} Sub['{sub}'] cidr:{subnet}",
                prev_sub_name = prev_subnet.subnet_name,
                prev_index = format!("{:2}/b{:2}",prev_subnet.src_index,prev_subnet.block_id).red(),
                prev_sub = prev_subnet.subscription_name.red(),
                index = format!("{:2}/b{:2}",s.src_index, s.block_id).blue(),
                sub = s.subscription_name.blue(),
                cidr_prev = cidr,
                subnet = s.subnet_cidr.as_ref().unwrap(),
                subnet_name = s.subnet_name
            );
            if prev_subnet.subnet_name == s.subnet_name
                && prev_subnet.subscription_name == s.subscription_name
            {
                log::warn!("Removing matching duplicate: {}", msg_duplicate);
                std::thread::sleep(std::time::Duration::from_millis(
                    crate::config::SLEEP_MSEC * 1,
                ));
                data.data.remove(i);
                continue;
            } else {
                panic!("{} {}", "Panic".on_red(), msg_duplicate);
            }
        }
        cidr = s.subnet_cidr.unwrap();
        prev_subnet = data.data[i].clone();
        i += 1;
    }
    Ok(data)
}

// Test the de_duplicate_subnets function
#[cfg(test)]
mod tests {
    use log4rs::filter;

    use super::*;
    use crate::graph_read_subnet_data::read_subnet_cache;
    // fn gen_cache_data() -> graph_read_subnet_data::Data {
    //     graph_read_subnet_data::Data {
    //         data: vec![Subnet::default()],
    //         skip_token: None,
    //         total_records: None,
    //         count: 0,
    //     }
    // }

    #[test]
    fn test_de_duplicate_subnets_one() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_01.json"))
            .expect("Error reading subnet cache");
        let result = de_duplicate_subnets(data, None).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            1,
            "Expected 1 subnets after de-duplication"
        );
        assert_eq!(result.data[0].subnet_name, "env-logs-crm-appgw-subnet");
    }
        #[test]
    fn test_de_duplicate_subnets_empty() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_03.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            3,
            "Expected 3 subnets before de-duplication"
        );
        // 1st filterd subnet = null, 2nd subnet = 10.0.0.0/24 Default
        let result = de_duplicate_subnets(data, None).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            1,
            "Expected 1 subnets after de-duplication"
        );
    }
    #[test]
    fn test_de_duplicate_subnets_multi() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_02.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            177,
            "Expected 177 subnets before de-duplication"
        );
        // Replace default subnet filter list
        let filter = vec![
            "default",
            "pkrsn1ooslfxj77", // Once in data
            "pkrsnsnajtq3h3i", // Not in data
            "pkrsnxocivqofa6", // Not in data
            "ORGgmcmg",        // Once in data
        ];
        let result = de_duplicate_subnets(data,Some(filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            169,
            "Expected 169 subnets after de-duplication"
        );
        // Verify this is expected dataset
        assert_eq!(result.data[151].subnet_name, "vm-mssql-cluster-a-subnet");

    }
        #[test]
    fn test_de_duplicate_subnets_multi_03() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_04.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            180,
            "Expected 177 subnets before de-duplication"
        );
        // Replace default subnet filter list
        let filter = vec![
            "default",
            "pkrsn1ooslfxj77", // Once in data
            "pkrsnsnajtq3h3i", // Not in data
            "pkrsnxocivqofa6", // Not in data
            "orggmcmg",        // Once in data
        ];
        let result = de_duplicate_subnets(data,Some(filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            173,
            "Expected 173 subnets after de-duplication"
        );
        // Verify this is expected dataset
        assert_eq!(result.data[151].subnet_name, "prod-fax-subnet");

    }
}
