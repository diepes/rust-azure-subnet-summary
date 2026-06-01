//! Azure CLI and Graph API interaction.
//!
//! This module handles all Azure-related operations:
//! - [`cli`] - Command execution for Azure CLI
//! - [`cache`] - Caching of subnet data
//! - [`graph`] - Azure Resource Graph queries

mod azure_cache;
mod cache;
mod cli;
mod fetch;
mod graph;
mod local_gateway;
mod local_gateway_cache;
mod paginate;
mod peering_cache;
mod peering_graph;
mod vwan_cache;
mod vwan_graph;

pub use fetch::{fetch_azure_data, AzureData, FetchConfig};

pub use azure_cache::CacheResult;
pub use cache::{read_subnet_cache, read_subnet_cache_with_status};
pub use cli::run;
pub use graph::{run_az_cli_graph, Data};
pub use local_gateway::{LocalGatewayData, LocalGatewayRow};
pub use local_gateway_cache::{read_local_gateway_cache, read_local_gateway_cache_with_status};
pub use peering_cache::{read_peering_cache, read_peering_cache_with_status};
pub use peering_graph::{PeeringData, PeeringEdge};
pub use vwan_cache::{read_vwan_cache, read_vwan_cache_with_status};
pub use vwan_graph::{VWanData, VWanRow};
