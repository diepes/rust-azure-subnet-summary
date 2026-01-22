//! Subnet data processing logic.
//!
//! This module contains business logic for processing subnet data:
//! - [`dedup`] - De-duplication of subnet records
//! - [`gap_finder`] - Finding gaps between subnets
//! - [`vnet`] - VNet aggregation and operations

mod dedup;
mod gap_finder;
mod vnet;

// Re-export public functions
pub use dedup::de_duplicate_subnets;
pub use gap_finder::{process_subnet_row, SubnetPrintRow};
pub use vnet::{get_vnets, print_vnets};
