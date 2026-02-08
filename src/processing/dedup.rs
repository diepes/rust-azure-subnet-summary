//! Subnet de-duplication logic.
//!
//! Handles removing duplicate and unwanted subnet entries.

use crate::azure::Data;
use std::error::Error;

/// Default list of subnet names to ignore during processing.
fn default_subnet_names_to_ignore() -> Vec<&'static str> {
    vec![
        "default",
        "jenkinsarm-snet",
        "pkrsn1ooslfxj77",
        "pkrsn8jufz9plf6",
        "pkrsnsnajtq3h3i",
        "pkrsnxocivqofa6",
        "orggmcmg",
        "restore-vm-subnet",
    ]
}

/// De-duplicate subnets by CIDR and subscription ID.
///
/// # Arguments
/// * `data` - The subnet data to de-duplicate
/// * `subnet_names_to_ignore` - Optional list of subnet names to filter out
///
/// # Returns
/// * `Ok(Data)` - De-duplicated data
pub fn de_duplicate_subnets(
    mut data: Data,
    subnet_names_to_ignore: Option<&Vec<&str>>,
) -> Result<Data, Box<dyn Error>> {
    let default_ignore_list = default_subnet_names_to_ignore();
    let subnet_names_to_ignore = subnet_names_to_ignore.unwrap_or(&default_ignore_list);

    // Filter out subnets with names that match the ignore list
    data.data.retain(|s| {
        !subnet_names_to_ignore.contains(&s.subnet_name.as_str()) && s.subnet_cidr.is_some()
    });

    // Dedup data.data - must be sorted first
    data.data
        .sort_by_key(|s| (s.subnet_cidr, s.subscription_id.clone()));
    data.data
        .dedup_by_key(|s| (s.subnet_cidr, s.subscription_id.clone()));

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::read_subnet_cache;
    use crate::get_sorted_subnets;

    #[test]
    fn test_de_duplicate_subnets_one() {
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_01.json"))
            .expect("Error reading subnet cache");
        let result = de_duplicate_subnets(data, None).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            1,
            "Expected 1 subnet after de-duplication"
        );
        assert_eq!(result.data[0].subnet_name, "env-logs-crm-appgw-subnet");
    }

    #[test]
    fn test_de_duplicate_subnets_empty() {
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_03.json"))
            .expect("Error reading subnet cache");
        let data_sorted = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_03.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            3,
            "Expected 3 subnets before de-duplication"
        );

        let result = de_duplicate_subnets(data, None).expect("Failed to de-duplicate subnets");
        let result_sorted =
            de_duplicate_subnets(data_sorted, None).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            1,
            "Expected 1 subnet after de-duplication"
        );
        assert_eq!(
            result_sorted.data.len(),
            1,
            "Expected 1 subnet after de-duplication"
        );
    }

    #[test]
    fn test_de_duplicate_subnets_multi() {
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_02.json"))
            .expect("Error reading subnet cache");
        assert_eq!(
            data.data.len(),
            177,
            "Expected 177 subnets before de-duplication"
        );

        let filter = vec![
            "default",
            "pkrsn1ooslfxj77",
            "pkrsnsnajtq3h3i",
            "pkrsnxocivqofa6",
            "ORGgmcmg",
        ];
        let result =
            de_duplicate_subnets(data, Some(&filter)).expect("Failed to de-duplicate subnets");
        assert_eq!(
            result.data.len(),
            158,
            "Expected 158 subnets after de-duplication"
        );
        assert_eq!(result.data[151].subnet_name, "z-ilt-lab4-snet-lnc-01");
    }

    #[test]
    fn test_de_duplicate_subnets_multi_04() {
        let data = read_subnet_cache(Some("src/tests/test_data/subnet_test_cache_04.json"))
            .expect("Error reading subnet cache");
        let data_sorted = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_04.json"))
            .expect("Error reading subnet cache");
        assert_eq!(data.data.len(), data_sorted.data.len());
        assert_eq!(data.data.len(), 180);

        let filter = vec![
            "default",
            "pkrsn1ooslfxj77",
            "pkrsnsnajtq3h3i",
            "pkrsnxocivqofa6",
            "orggmcmg",
        ];
        let result =
            de_duplicate_subnets(data, Some(&filter)).expect("Failed to de-duplicate subnets");
        let result_sorted = de_duplicate_subnets(data_sorted, Some(&filter))
            .expect("Failed to de-duplicate subnets");
        assert_eq!(result.data.len(), result_sorted.data.len());
        assert_eq!(result.data.len(), 159);
        assert_eq!(result.data[151].subnet_name, "z-ilt-lab5-snet-adds-01");
    }
}
