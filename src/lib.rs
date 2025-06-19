// cargo watch -x 'fmt' -x 'run'  // 'run -- --some-arg'

//use crate::struct_subnet::Subnet;
//use ipv4::{get_cidr_mask_ipv4, Ipv4};
mod cmd;
mod config;
mod graph_read_subnet_data;
pub mod struct_vnet;
use std::{collections::HashSet, net::Ipv4Addr};

use colored::Colorize;
use struct_vnet::VnetList;
pub mod struct_subnet;
use struct_subnet::Subnet;
pub mod ipv4;
use ipv4::Ipv4;
pub mod print_subnets;
//mod read_csv;
mod write_banner;

pub fn get_sorted_subnets() -> Result<graph_read_subnet_data::Data, Box<dyn std::error::Error>> {
    let mut data = graph_read_subnet_data::read_subnet_cache(None).expect("Error running az cli graph");
    // Sort by subnet_cidr
    data.data.sort_by_key(|s| s.subnet_cidr);
    Ok(data)
}

pub fn get_vnets(
    data: &graph_read_subnet_data::Data,
) -> Result<VnetList, Box<dyn std::error::Error>> {
    let mut vnets = VnetList::new();
    vnets.add_vnet(data.data.first().unwrap());
    // = data.data.iter().map(|s| s.vnet_name.clone()).collect();
    Ok(vnets)
}

pub fn check_for_duplicate_subnets(
    data: &graph_read_subnet_data::Data,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut seen = HashSet::new();

    for sub in data.data.iter() {
        if !seen.insert((sub.subnet_cidr.clone(), sub.subscription_id.clone())) {
            return Err(format!("Duplicate found: {:?}", sub).into());
        }
    }
    Ok(())
}
pub fn de_duplicate_subnets(
    mut data: graph_read_subnet_data::Data,
) -> Result<graph_read_subnet_data::Data, Box<dyn std::error::Error>> {
    // Check for duplicate subnet_cidr
    let mut cidr: Ipv4 = Ipv4::new("11.2.0.0/32").unwrap();
    let mut prev_subnet = Subnet::default();
    let mut dup = 9999;
    // loop through the subnets checking for duplicate subnet_cidr
    // this is not very efficient but the data is small
    //for (i, s) in data.data.iter().enumerate() {
    let mut i = 0;
    while i < data.data.len() {
        let s = &data.data[i];
        if (s.src_index + s.block_id * 1000) == dup {
            panic!(" Duplicate src_index: {}", dup);
        }
        dup = s.src_index + s.block_id * 1000;
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
            // pause for 2 seconds to allow user to read message
            std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 2));
            data.data.remove(i);
            continue;
        };
        let subnet_names_to_ignore = [
            "default",
            "jenkinsarm-snet",
            "pkrsn1ooslfxj77",
            "pkrsn8jufz9plf6",
            "pkrsnsnajtq3h3i",
            "pkrsnxocivqofa6",
            "twggmcmg",
            "restore-vm-subnet",
        ]; // "HPAEMCMGTWG"
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

            // pause for 2 seconds to allow user to read message
            std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 1));
            data.data.remove(i);
            continue;
        };
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
            // Found a duplicate, we could ignore or panic!
            let msg_duplicate = format!(
                "Duplicate cidr:{cidr},\n    Name[{prev_index}]{prev_sub_name} Sub['{prev_sub}'] cidr:{cidr_prev}\n    Name[{index}]{subnet_name} Sub['{sub}'] cidr:{subnet}",
                prev_sub_name = prev_subnet.subnet_name,
                prev_index = format!("{:2}/b{:2}",prev_subnet.src_index,prev_subnet.block_id).red(),
                prev_sub = prev_subnet.subscription_name.red(),
                index = format!("{:2}/b{:2}",s.src_index, s.block_id).blue(),
                sub = s.subscription_name.blue(),
                cidr_prev = cidr,
                subnet = s.subnet_cidr.as_ref().unwrap(),
                subnet_name = s.subnet_name
            );
            // Check if subnet name and subscription name are the same ignore as duplicate graph error
            if prev_subnet.subnet_name == s.subnet_name
                && prev_subnet.subscription_name == s.subscription_name
            {
                log::warn!("Removing matching duplicate: {}", msg_duplicate);
                // pause for 5 seconds to allow user to read message
                std::thread::sleep(std::time::Duration::from_millis(config::SLEEP_MSEC * 1));
                data.data.remove(i);
                continue;
            } else {
                panic!("{} {}", "Panic".on_red(), msg_duplicate);
            }
        }
        cidr = s.subnet_cidr.unwrap();
        prev_subnet = data.data[i].clone();
        i += 1; // next subnet
    }
    Ok(data)
}

fn _escape_csv_field(input: &str) -> String {
    if input.contains(',') || input.contains('"') {
        // If the string contains a comma or double quote, enclose it in double quotes
        // and escape any double quotes within the field.
        // also excel does not like spaces after comma between fields
        let escaped = input.replace("\"", "\"\"");
        format!("\"{}\"", escaped)
    } else {
        // If the string doesn't contain a comma or double quote, no need to enclose it.
        input.to_string()
    }
}
