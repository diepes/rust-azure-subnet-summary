//! Azure CLI and Graph API interaction.
//!
//! This module handles all Azure-related operations:
//! - [`cli`] - Command execution for Azure CLI
//! - [`cache`] - Caching of subnet data
//! - [`graph`] - Azure Resource Graph queries

mod cache;
mod cli;
mod graph;

// Re-export public types and functions
pub use cache::read_subnet_cache;
pub use cli::run;
pub use graph::{run_az_cli_graph, Data};
