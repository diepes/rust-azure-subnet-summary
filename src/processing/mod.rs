//! Subnet data processing logic.
//!
//! This module contains business logic for processing subnet data:
//! - [`dedup`] - De-duplication of subnet records
//! - [`gap_finder`] - Finding gaps between subnets
//! - [`vnet`] - VNet aggregation and operations
//! - [`overlap`] - Detection and filtering of overlapping VNet CIDRs

mod dedup;
pub(crate) mod gap_finder;
mod overlap;
mod vnet;

// Re-export public functions
pub use dedup::de_duplicate_subnets;
pub use gap_finder::{
    fill_trailing_vgap, gaps, process_subnet_row, GapEvent, GapFinder, GapKind, PrevVnetContext,
    SubnetPrintRow, VnetCidr,
};
pub use overlap::{
    find_overlapping_vnets, log_overlapping_vnets, resolve_overlapping_vnets,
    ConflictResolutionOutput, ExcludedSubnet, OverlapConflict, VnetInfo,
};
pub use vnet::{get_vnets, print_vnets};
