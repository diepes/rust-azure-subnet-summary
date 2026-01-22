// src/de_duplicate_subnets.rs

use crate::graph_read_subnet_data;
use std::error::Error;
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
// try2 deduplicate subnets by cidr and subscription_id
pub fn de_duplicate_subnets2(
    mut data: graph_read_subnet_data::Data,
    subnet_names_to_ignore: Option<&Vec<&str>>,
) -> Result<graph_read_subnet_data::Data, Box<dyn Error>> {
    // if subnet_names_to_ignore is None, use the default list
    let subnet_names_to_ignore = match subnet_names_to_ignore {
        Some(names) => names,
        None => &default_subnet_names_to_ignore(),
    };
    // Filter out sunets with names that match the ignore list
    data.data.retain(|s| {
        !subnet_names_to_ignore.contains(&s.subnet_name.as_str()) && s.subnet_cidr.is_some()
    });
    // dedup data.data, it has to be sorted first
    data.data
        .sort_by_key(|s| (s.subnet_cidr, s.subscription_id.clone()));
    data.data
        .dedup_by_key(|s| (s.subnet_cidr, s.subscription_id.clone()));

    Ok(data)
}

// Test the de_duplicate_subnets function
#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        get_sorted_subnets_legacy as get_sorted_subnets, graph_read_subnet_data::read_subnet_cache,
    };

    #[test]
    fn test_de_duplicate_subnets_one() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_01.json"))
            .expect("Error reading subnet cache");
        let result = de_duplicate_subnets2(data, None).expect("Failed to de-duplicate subnets");
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
        let data_sorted = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_03.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            3,
            "Expected 3 subnets before de-duplication"
        );
        // 1st filterd subnet = null, 2nd subnet = 10.0.0.0/24 Default
        let result = de_duplicate_subnets2(data, None).expect("Failed to de-duplicate subnets");
        let result_sorted =
            de_duplicate_subnets2(data_sorted, None).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            1,
            "Expected 1 subnets after de-duplication"
        );
        assert_eq!(
            result_sorted.data.len(),
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
        let result =
            de_duplicate_subnets2(data, Some(&filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            158,
            "Expected 158 subnets after de-duplication"
        );
        // Verify this is expected dataset
        assert_eq!(result.data[151].subnet_name, "z-ilt-lab4-snet-lnc-01");
    }
    #[test]
    fn test_de_duplicate_subnets_multi_04() {
        //let mut data = gen_cache_data();
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_04.json"))
            .expect("Error reading subnet cache");
        let data_sorted = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_04.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            data_sorted.data.len(),
            "Expected same number of subnets before de-duplication"
        );
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
        let result =
            de_duplicate_subnets2(data, Some(&filter)).expect("Failed to de-duplicate subnets");
        let result_sorted = de_duplicate_subnets2(data_sorted, Some(&filter))
            .expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            result_sorted.data.len(),
            "Expected same number of subnets after de-duplication"
        );
        assert_eq!(
            result.data.len(),
            159,
            "Expected 159 subnets after de-duplication"
        );
        // Verify this is expected dataset
        assert_eq!(result.data[151].subnet_name, "z-ilt-lab5-snet-adds-01");
    }
}
