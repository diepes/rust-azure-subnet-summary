//! Output formatting for subnet data.
//!
//! This module handles formatting and outputting subnet data:
//! - [`csv`] - CSV output formatting
//! - [`dup_report`] - Markdown duplicate VNet report
//! - [`terminal`] - Terminal output with colors

mod csv;
mod dup_report;
mod peering_diagram;
mod peering_dot;
mod peering_topology;
mod terminal;

pub use csv::subnet_print;
pub use dup_report::write_duplicates_md;
pub use peering_diagram::write_peering_diagram;
pub use peering_dot::write_peering_dot;
pub use terminal::format_field;
