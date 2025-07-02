// cargo watch -x 'fmt' -x 'run'  // 'run -- --some-arg'

//use crate::subnet_struct::Subnet;
//use ipv4::{get_cidr_mask_ipv4, Ipv4};
mod cmd;
mod config;
mod de_duplicate_subnets;
mod graph_read_subnet_data;
mod ipv4;
pub mod struct_vnet;
mod subnet_struct;
use std::collections::HashSet;

use struct_vnet::VnetList;
pub mod subnet_add_row;
pub mod subnet_print;
mod write_banner;

pub fn get_sorted_subnets(
    cache_file: Option<&str>,
) -> Result<graph_read_subnet_data::Data, Box<dyn std::error::Error>> {
    let mut data =
        graph_read_subnet_data::read_subnet_cache(cache_file).expect("Error running az cli graph");
    // Sort by subnet_cidr
    data.data.sort_by_key(|s| s.subnet_cidr);
    Ok(data)
}

// Remove get_vnets from lib.rs and re-export from struct_vnet
pub use struct_vnet::get_vnets;
// return error if duplicate subnets found
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
pub use de_duplicate_subnets::de_duplicate_subnets2;

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
