//! Output formatting for subnet data.
//!
//! This module handles formatting and outputting subnet data:
//! - [`csv`] - CSV output formatting
//! - [`dup_report`] - Markdown duplicate VNet report
//! - [`terminal`] - Terminal output with colors
//! - [`validate_dot`] - Pre-render validation of generated DOT files

mod csv;
mod dup_report;
mod peering_diagram;
mod peering_dot;
mod peering_topology;
mod terminal;
pub mod validate_dot;

pub use csv::subnet_print;
pub use dup_report::write_duplicates_md;
pub use peering_diagram::write_peering_diagram;
pub use peering_dot::write_peering_dot;
pub use peering_topology::{build_topology, PeeringTopology};
pub use terminal::format_field;
pub use validate_dot::validate_dot_file;
