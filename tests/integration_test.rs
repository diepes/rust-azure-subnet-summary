//! Integration tests for azure-subnet-summary
//!
//! These tests verify the complete workflow from reading cache to processing.

use azure_subnet_summary::{
    check_for_duplicate_subnets, get_sorted_subnets,
    processing::{de_duplicate_subnets, get_vnets},
};

#[test]
fn test_full_workflow_with_cache() {
    // Read from test cache
    let data = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_04.json"))
        .expect("Failed to read subnet cache");

    assert_eq!(data.data.len(), 180, "Expected 180 subnets in test data");

    // De-duplicate
    let filter = vec![
        "default",
        "pkrsn1ooslfxj77",
        "pkrsnsnajtq3h3i",
        "pkrsnxocivqofa6",
        "orggmcmg",
    ];
    let data = de_duplicate_subnets(data, Some(&filter)).expect("Failed to de-duplicate");

    assert_eq!(data.data.len(), 159, "Expected 159 subnets after de-dup");

    // Check for duplicates
    check_for_duplicate_subnets(&data).expect("Found unexpected duplicates");

    // Get VNets
    let vnets = get_vnets(&data).expect("Failed to get VNets");
    assert!(!vnets.vnets.is_empty(), "Should have VNets");
}

#[test]
fn test_small_cache_file() {
    let data = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_01.json"))
        .expect("Failed to read subnet cache");

    let data = de_duplicate_subnets(data, None).expect("Failed to de-duplicate");

    assert_eq!(data.data.len(), 1, "Expected 1 subnet after de-dup");
    assert_eq!(data.data[0].subnet_name, "env-logs-crm-appgw-subnet");
}

#[test]
fn test_sorted_order() {
    let data = get_sorted_subnets(Some("src/tests/test_data/subnet_test_cache_04.json"))
        .expect("Failed to read subnet cache");

    // Verify subnets are sorted by CIDR
    for i in 1..data.data.len() {
        let prev = &data.data[i - 1];
        let curr = &data.data[i];
        if let (Some(prev_cidr), Some(curr_cidr)) = (prev.subnet_cidr, curr.subnet_cidr) {
            assert!(
                prev_cidr <= curr_cidr,
                "Subnets should be sorted: {:?} > {:?}",
                prev_cidr,
                curr_cidr
            );
        }
    }
}
