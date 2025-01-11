use azure_graph::ipv4::Ipv4;
// use azure_mgmt_compute::models::VirtualMachine;
//use azure_graph::{self, az}; // Import lib.rs (library)
use log4rs;
//use tokio::sync::broadcast;
//use tokio::main;
use std::error::Error;
use std::net::Ipv4Addr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Do as little as possible in main.rs as it can't contain any tests
    log4rs::init_file("log4rs.yml", Default::default()).expect("Error initializing log4rs");
    dotenv::dotenv().ok();
    //
    log::info!("#Start main()");

    // let graph = az::graph_rs_sdk::paging().await;
    // if let Err(e) = graph {
    //     log::error!("Error: {:?}", e);
    // }

    let data = azure_graph::get_subnet_fill_gaps().expect("Error running az cli graph");
    //log::info!("# output = {:#?}", output);
    log::info!("# Got subnet count = {} == {}", data.count, data.data.len());
    println!(
        r#""cnt","gap","subnet_cidr","broadcast","subnet_name","subscription_name","vnet_cidr","vnet_name","location","nsg","dns","subscription_id""#
    );
    const DEFAULT_CIDR_MASK: u8 = 28; // /28 = 11 ips for hosts in Azure. (16-5)
    const SKIP_SUBNET_SMALLER_THAN: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);
    let mut next_ip = Ipv4::new("0.0.0.0/24")?;
    let mut vnet_previous_cidr = Ipv4::new("0.0.0.0/24")?;
    for (i, s) in data.data.iter().enumerate() {
        let subnet_cidr: Ipv4;
        match s.subnet_cidr {
            Some(s) => {
                subnet_cidr = s;
            }
            None => {
                log::warn!(
                    "Warning: subnet_cidr is None for subnet_name: {}",
                    s.subnet_name
                );
                println!(
                    r#""{j}","{gap}","{subnet_cidr}","{broadcast}({az_hosts}vm)","{subnet_name}","{subscription_name}","{vnet_cidr}","{vnet_name}","{location}","{nsg}","{dns}","{subscription_id}""#,
                    j = i + 1,
                    gap = "None", // Subnet missing cidr
                    subnet_name = s.subnet_name,
                    subnet_cidr = "none",
                    broadcast = "none",
                    az_hosts = 0,
                    vnet_name = s.vnet_name,
                    vnet_cidr = s
                        .vnet_cidr
                        .iter()
                        .map(|ip| ip.to_string())
                        .collect::<Vec<String>>()
                        .join(","),
                    location = s.location,
                    nsg = s
                        .nsg
                        .as_ref()
                        .unwrap_or(&"None".to_string())
                        .split("/")
                        .last()
                        .unwrap(),
                    dns = s
                        .dns_servers
                        .as_ref()
                        .unwrap_or(&vec!["None".to_string()])
                        .join(","),
                    subscription_name = s.subscription_name,
                    subscription_id = s.subscription_id,
                );
                continue;
            }
        }
        while next_ip.addr > SKIP_SUBNET_SMALLER_THAN
            && next_ip.addr < subnet_cidr.addr  // ignore mask
            && next_ip < subnet_cidr
            && next_ip >= vnet_previous_cidr // Stay above vnet start
            && azure_graph::ipv4::broadcast_addr_ipv4(next_ip)? < azure_graph::ipv4::broadcast_addr_ipv4(vnet_previous_cidr)?
            && next_ip.addr.octets()[0] == s.vnet_cidr[0].addr.octets()[0]
        // same first octet e.g. 10. != 172.
        {
            // reduce mask if we jumped over smaller subnet
            let mut next_ip_broadcast = azure_graph::ipv4::broadcast_addr_ipv4(next_ip)?;
            if next_ip_broadcast >= subnet_cidr {
                next_ip.mask = subnet_cidr.mask;
                next_ip_broadcast = azure_graph::ipv4::broadcast_addr_ipv4(next_ip)?;
                if next_ip_broadcast >= subnet_cidr {
                    panic!("Gap bigger than subnet, after mask reduction !!! next_ip_broadcast:{:?} subnet:{}  next_ip{}", next_ip_broadcast, subnet_cidr, next_ip)
                }
            }
            println!(
                r#""---","gap","{subnet_cidr}","{broadcast}({az_hosts}vm)","{subnet_name}","{subscription_name}","{vnet_cidr}","{vnet_name}","{location}","{nsg}","{dns}","{subscription_id}""#,
                //"gap     =      {}  sub_cidr: {sub} , vnet_cidr: {vnet}",
                subnet_cidr = next_ip,
                broadcast = next_ip_broadcast.addr,
                az_hosts = azure_graph::ipv4::num_az_hosts(next_ip.mask)?,
                subnet_name = "None",
                vnet_name = s.vnet_name,
                vnet_cidr = s
                    .vnet_cidr
                    .iter()
                    .map(|ip| ip.to_string())
                    .collect::<Vec<String>>()
                    .join(","),
                location = "None",
                nsg = "None",
                dns = "None",
                subscription_name = s.subscription_name,
                subscription_id = s.subscription_id,
            );
            //
            // Trap gaps that are rolling into next subnet or out of vnet.
            let vnet_broadcast_max = if s.vnet_cidr[0] == vnet_previous_cidr {
                azure_graph::ipv4::broadcast_addr_ipv4(s.vnet_cidr[0])?
            } else {
                s.vnet_cidr[0]
            };
            if next_ip_broadcast > vnet_broadcast_max || next_ip_broadcast >= subnet_cidr {
                if next_ip_broadcast >= vnet_broadcast_max {
                    log::error!(
                        "next_ip_broadcast[{}] >= vnet_broadcast_max[{}]   ... next_ip:[{}]",
                        next_ip_broadcast,
                        vnet_broadcast_max,
                        next_ip,
                    );
                }
                if next_ip_broadcast >= subnet_cidr {
                    log::error!(
                        "next_ip_broadcast[{}] >= s.subnet_cidr[{}]... next_ip:[{}]",
                        next_ip_broadcast,
                        subnet_cidr,
                        next_ip,
                    );
                }
                panic!("Gap bigger than subnet or vnet !!! next:{:?} vnet:{:?} following_subnet:{:?} previous_vnet: {:?}", next_ip_broadcast, s.vnet_cidr[0], subnet_cidr, vnet_previous_cidr)
            }
            next_ip = azure_graph::ipv4::next_subnet_ipv4(next_ip, Some(DEFAULT_CIDR_MASK))?;
            // next_ip.addr + 1
        }
        // println!("next_ip    =    {}", next_ip);
        println!(
            r#""{j}","{gap}","{subnet_cidr}","{broadcast}({az_hosts}vm)","{subnet_name}","{subscription_name}","{vnet_cidr}","{vnet_name}","{location}","{nsg}","{dns}","{subscription_id}""#,
            j = i + 1,
            gap = s.gap.as_ref().unwrap_or(&format!("Sub{}", s.src_index)),
            subnet_name = s.subnet_name,
            subnet_cidr = subnet_cidr,
            broadcast = azure_graph::ipv4::broadcast_addr_ipv4(subnet_cidr)?.addr,
            az_hosts = azure_graph::ipv4::num_az_hosts(subnet_cidr.mask)?,
            vnet_name = s.vnet_name,
            vnet_cidr = s
                .vnet_cidr
                .iter()
                .map(|ip| ip.to_string())
                .collect::<Vec<String>>()
                .join(","),
            location = s.location,
            nsg = s
                .nsg
                .as_ref()
                .unwrap_or(&"None".to_string())
                .split("/")
                .last()
                .unwrap(),
            dns = s
                .dns_servers
                .as_ref()
                .unwrap_or(&vec!["None".to_string()])
                .join(","),
            subscription_name = s.subscription_name,
            subscription_id = s.subscription_id,
        );
        vnet_previous_cidr = s.vnet_cidr[0];
        if subnet_cidr.mask < 29 {
            // keep current mask size
            // /28 11 ips
            // next_ip = azure_graph::ipv4::next_subnet_ipv4(subnet_cidr, None)?;
            next_ip = azure_graph::ipv4::next_subnet_ipv4(subnet_cidr, Some(28))?;
        } else {
            next_ip = azure_graph::ipv4::next_subnet_ipv4(subnet_cidr, Some(28))?;
        }
    }
    println!(
        "# End main() Skipped subnet smaller than {:?}",
        SKIP_SUBNET_SMALLER_THAN
    );

    Ok(())
}
