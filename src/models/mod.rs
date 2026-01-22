//! Domain models for Azure subnet summary.
//!
//! This module contains the core data structures used throughout the application:
//! - [`Ipv4`] - IPv4 address with CIDR notation support
//! - [`Subnet`] - Azure subnet representation
//! - [`Vnet`] and [`VnetList`] - Virtual network structures

mod ipv4;
mod subnet;
mod vnet;

// Re-export public types
pub use ipv4::{
    broadcast_addr, cut_addr, cut_addr_ipv4, get_cidr_mask, get_cidr_mask_ipv4, ip_after_subnet,
    lo_mask, next_subnet_ipv4, num_az_hosts, Ipv4, MAX_LENGTH,
};
pub use subnet::Subnet;
pub use vnet::{Vnet, VnetList};
