use crate::ipv4::Ipv4;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Subnet {
    pub vnet_name: String,
    pub vnet_cidr: Vec<Ipv4>,
    pub subnet_name: String,
    pub subnet_cidr: Option<Ipv4>,
    pub nsg: Option<String>,
    pub location: String,
    pub dns_servers: Option<Vec<String>>,
    pub subscription_id: String,
    pub subscription_name: String,
    pub ip_configurations_count: Option<u32>,
    //  "ip_configurations": [
    //     {
    //       "id": "/subscriptions/c4855b85-e4fb-4ae6-9db6-34dc74d21cc4/resourceGroups/DR-VNET-RG/providers/Microsoft.Network/virtualHubs/Z-DR-HUB-ARS-GBGKFC/ipConfigurations/IPCONFIG1",
    //       "resourceGroup": "DR-VNET-RG"
    //     },
    // Fill value to gap if we create new subnet
    pub gap: Option<String>,
    // Serde field to ignore and set default value
    #[serde(default)] // skip_deserializing use default for graph query but return for cache
    pub src_index: usize, // record index from source
    #[serde(default)] // skip_deserializing use default for graph query but return for cache
    pub block_id: usize, // This field will be manually assigned as graph returns blocks of 50
}
impl Default for Subnet {
    fn default() -> Self {
        Subnet {
            vnet_name: "blank".to_string(),
            vnet_cidr: vec![],
            subnet_name: "".to_string(),
            subnet_cidr: None,
            nsg: None,
            location: "blank".to_string(),
            dns_servers: None,
            subscription_id: "blank".to_string(),
            subscription_name: "blank".to_string(),
            ip_configurations_count: None,
            gap: Some("blank".to_string()),
            src_index: 0,
            block_id: 0,
        }
    }
}
