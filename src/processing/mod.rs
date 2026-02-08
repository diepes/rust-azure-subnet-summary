//! Subnet data processing logic.
//!
//! This module contains business logic for processing subnet data:
//! - [`dedup`] - De-duplication of subnet records
//! - [`gap_finder`] - Finding gaps between subnets
//! - [`vnet`] - VNet aggregation and operations
//! - [`overlap`] - Detection and filtering of overlapping VNet CIDRs

mod dedup;
mod gap_finder;
mod overlap;
mod vnet;

// Re-export public functions
pub use dedup::de_duplicate_subnets;
pub use gap_finder::{process_subnet_row, SubnetPrintRow};
pub use overlap::{
    filter_excluded_vnet_cidrs, filter_overlapping_vnets, find_overlapping_vnets,
    get_excluded_vnets, log_overlapping_vnets, OverlapConflict, VnetInfo,
};
pub use vnet::{get_vnets, print_vnets};
