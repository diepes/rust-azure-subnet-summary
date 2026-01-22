//! IPv4 functions (legacy module - use models::ipv4 for new code)
#![allow(dead_code)]

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use std::error::Error;
use std::net::Ipv4Addr;
use std::str::FromStr;

pub const MAX_LENGTH: u8 = 32;

pub fn get_cidr_mask_ipv4(ipv4: Ipv4) -> Result<u32, Box<dyn Error>> {
    get_cidr_mask(ipv4.mask)
}
pub fn get_cidr_mask(len: u8) -> Result<u32, Box<dyn Error>> {
    if len > MAX_LENGTH {
        Err("Network length is too long".into())
    } else {
        let right_len = MAX_LENGTH - len;
        let all_bits = u32::MAX as u64;

        let mask = (all_bits >> right_len) << right_len;

        Ok(mask as u32)
    }
}

pub fn cut_addr_ipv4(ipv4: Ipv4, len: u8) -> Result<Ipv4, Box<dyn Error>> {
    if len <= ipv4.mask {
        Err("Network can only be cut to a smaller size".into())
    } else {
        let ipv4_addr = cut_addr(ipv4.addr, len)?;
        Ok(Ipv4 {
            addr: ipv4_addr,
            mask: len,
        })
    }
}

pub fn cut_addr(addr: Ipv4Addr, len: u8) -> Result<Ipv4Addr, Box<dyn Error>> {
    if len > MAX_LENGTH {
        Err("Network length is too long".into())
    } else {
        let right_len = MAX_LENGTH - len;
        let bits = u32::from(addr) as u64;
        let new_bits = (bits >> right_len) << right_len;

        Ok(Ipv4Addr::from(new_bits as u32))
    }
}

pub fn next_subnet_ipv4(ipv4: Ipv4, mask: Option<u8>) -> Result<Ipv4, Box<dyn Error>> {
    let current_mask = ipv4.mask;
    let new_mask = mask.unwrap_or(current_mask);
    if new_mask <= current_mask {
        // eq or larger subnet (smaller mask)
        let next_subnet = ip_after_subnet(ipv4.addr, new_mask)?;
        Ok(Ipv4 {
            addr: next_subnet,
            mask: new_mask,
        })
    } else {
        //smaller subnet
        let current_broadcast = broadcast_addr(ipv4.addr, current_mask)?;
        let next_subnet = ip_after_subnet(current_broadcast, new_mask)?;
        Ok(Ipv4 {
            addr: next_subnet,
            mask: new_mask,
        })
    }
}

// Not accurate as original subnet mask not available.
/// Returns the ip address following the given subnet.
pub fn ip_after_subnet(addr: Ipv4Addr, cidr: u8) -> Result<Ipv4Addr, Box<dyn Error>> {
    if cidr > MAX_LENGTH {
        Err("Network length is too long".into())
    } else {
        let subnet_size = 1 << (MAX_LENGTH - cidr);
        let addr_bits = u32::from(addr);
        let network_bits = addr_bits & get_cidr_mask(cidr)?; // Mask the address to get the network part
        let next_subnet_bits = network_bits
            .checked_add(subnet_size)
            .ok_or("Next subnet calculation overflowed")?;
        Ok(Ipv4Addr::from(next_subnet_bits))
    }
}

pub fn broadcast_addr(addr: Ipv4Addr, len: u8) -> Result<Ipv4Addr, Box<dyn Error>> {
    if len > MAX_LENGTH {
        Err("Network length is too long".into())
    } else {
        let mask = get_cidr_mask(len)?;
        let addr_bits = u32::from(addr);
        let network_bits = addr_bits & mask;
        let broadcast_bits = network_bits | (!mask);
        Ok(Ipv4Addr::from(broadcast_bits))
    }
}
pub fn num_az_hosts(len: u8) -> Result<u64, Box<dyn Error>> {
    if len >= MAX_LENGTH - 2 {
        // /29 6 ip's one host
        Err("Network length is too long or invalid".into())
    } else {
        let num_az_hosts = (1u64 << (MAX_LENGTH - len)) - 5; // -5 for network, broadcast, and gateway + 2 dns
        Ok(num_az_hosts)
    }
}
pub fn lo_mask(ip: Ipv4Addr) -> u8 {
    // Convert IPv4 to u32 (big-endian)
    let ip_u32 = u32::from(ip);
    // Count binary trailing zeros
    let trailing_zeros = ip_u32.trailing_zeros() as u8;
    assert!(trailing_zeros <= 32, "Trailing zeros exceed 32 bits");
    32 - trailing_zeros
}
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Copy, Clone, Hash)]
pub struct Ipv4 {
    pub addr: Ipv4Addr,
    pub mask: u8,
}
impl Serialize for Ipv4 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let cidr = format!("{}/{}", self.addr, self.mask);
        serializer.serialize_str(&cidr)
    }
}
impl<'de> Deserialize<'de> for Ipv4 {
    fn deserialize<D>(deserializer: D) -> Result<Ipv4, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(de::Error::custom(format!("invalid CIDR format: {s}")));
        }

        let addr = Ipv4Addr::from_str(parts[0])
            .map_err(|_| de::Error::custom(format!("invalid IP address: {}", parts[0])))?;
        let mask = u8::from_str(parts[1])
            .map_err(|_| de::Error::custom(format!("invalid subnet mask: {}", parts[1])))?;

        Ok(Ipv4 { addr, mask })
    }
}
impl Ipv4 {
    pub fn new(addr_cidr: &str) -> Result<Ipv4, Box<dyn Error>> {
        let addr_cidr = addr_cidr.trim();
        let parts: Vec<&str> = addr_cidr.split('/').collect();
        if parts.len() != 2 {
            return Err("Invalid address/mask".into());
        }
        let addr = parts[0]
            .parse()
            .unwrap_or_else(|_| panic!("Invalid address {}", parts[0]));
        let mask = parts[1].parse()?;
        if mask > MAX_LENGTH {
            return Err("Network length is too long".into());
        }
        Ok(Ipv4 { addr, mask })
    }
    pub fn broadcast(&self) -> Result<Ipv4, Box<dyn Error>> {
        let broadcast = broadcast_addr(self.addr, self.mask)?;
        Ok(Ipv4 {
            addr: broadcast,
            mask: self.mask,
        })
    }
    pub fn hi(&self) -> Ipv4Addr {
        broadcast_addr(self.addr, self.mask)
            .unwrap_or_else(|e| panic!("Error calculating broadcast address: {e}"))
    }
    pub fn lo(&self) -> Ipv4Addr {
        cut_addr(self.addr, self.mask)
            .unwrap_or_else(|e| panic!("Error calculating minimum address for {self}: {e}"))
    }
    /// Check if an IP address is contained within this subnet.
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        ip >= self.lo() && ip <= self.hi()
    }
}
impl std::fmt::Display for Ipv4 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.mask)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cidr_mask() {
        assert_eq!(get_cidr_mask(0).unwrap(), 0x00000000);
        assert_eq!(get_cidr_mask(8).unwrap(), 0xFF000000);
        assert_eq!(get_cidr_mask(16).unwrap(), 0xFFFF0000);
        assert_eq!(get_cidr_mask(24).unwrap(), 0xFFFFFF00);
        assert_eq!(get_cidr_mask(32).unwrap(), 0xFFFFFFFF);

        assert!(get_cidr_mask(33).is_err());
    }

    #[test]
    fn test_cut_addr() {
        let ip = Ipv4Addr::new(192, 168, 1, 42);
        assert_eq!(cut_addr(ip, 24).unwrap(), Ipv4Addr::new(192, 168, 1, 0));
        assert_eq!(cut_addr(ip, 16).unwrap(), Ipv4Addr::new(192, 168, 0, 0));
        assert_eq!(cut_addr(ip, 8).unwrap(), Ipv4Addr::new(192, 0, 0, 0));
        assert_eq!(cut_addr(ip, 32).unwrap(), Ipv4Addr::new(192, 168, 1, 42));

        assert!(cut_addr(ip, 33).is_err());
    }

    #[test]
    fn test_next_subnet() {
        let ip = Ipv4Addr::new(192, 168, 1, 0);
        assert_eq!(
            ip_after_subnet(ip, 24).unwrap(),
            Ipv4Addr::new(192, 168, 2, 0)
        );
        assert_eq!(
            ip_after_subnet(ip, 16).unwrap(),
            Ipv4Addr::new(192, 169, 0, 0)
        );
        assert_eq!(ip_after_subnet(ip, 8).unwrap(), Ipv4Addr::new(193, 0, 0, 0));
        assert_eq!(
            ip_after_subnet(ip, 32).unwrap(),
            Ipv4Addr::new(192, 168, 1, 1)
        );

        assert!(ip_after_subnet(Ipv4Addr::new(255, 255, 255, 255), 24).is_err());
    }
    #[test]
    fn test_next_subnet_ipv4() {
        let ip1 = Ipv4::new("10.1.1.0/28").unwrap();
        assert_eq!(
            next_subnet_ipv4(ip1, None).unwrap(),
            Ipv4::new("10.1.1.16/28").unwrap()
        );

        let ip2 = Ipv4::new("10.1.1.0/29").unwrap();
        let ip2_next = next_subnet_ipv4(ip2, None).unwrap();
        assert_eq!(ip2_next, Ipv4::new("10.1.1.8/29").unwrap());
        assert_eq!(
            next_subnet_ipv4(ip2_next, None).unwrap(),
            Ipv4::new("10.1.1.16/29").unwrap()
        );
        assert_eq!(
            next_subnet_ipv4(
                Ipv4 {
                    addr: ip2_next.addr,
                    mask: 28
                },
                None
            )
            .unwrap(),
            Ipv4::new("10.1.1.16/28").unwrap()
        );
        // test with mask - chatgpt

        let ipv4 = Ipv4::new("192.168.1.0/8").unwrap();
        assert_eq!(
            ipv4.broadcast().unwrap(),
            Ipv4::new("192.255.255.255/8").unwrap()
        );
        assert_eq!(
            next_subnet_ipv4(ipv4, None).unwrap(),
            Ipv4::new("193.0.0.0/8").unwrap()
        );
        assert_eq!(
            (Ipv4::new("192.255.255.255/8").unwrap())
                .broadcast()
                .unwrap(),
            Ipv4::new("192.255.255.255/8").unwrap()
        );
        // moving from big subnet to smaller subnet
        let next_ipv4 = next_subnet_ipv4(ipv4, Some(16)).unwrap();
        assert_eq!(next_ipv4.mask, 16);
        assert_eq!(next_ipv4.addr, Ipv4Addr::new(193, 0, 0, 0));

        // test with mask
        let ip3 = Ipv4::new("10.2.3.4/16").unwrap();
        assert_eq!(
            next_subnet_ipv4(ip3, None).unwrap(),
            Ipv4::new("10.3.0.0/16").unwrap()
        );
        assert_eq!(
            next_subnet_ipv4(ip3, Some(24)).unwrap(),
            Ipv4::new("10.3.0.0/24").unwrap()
        );
        // seq of subnets
        let ip5 = Ipv4::new("10.18.126.0/24").unwrap();
        let next_ip5 = next_subnet_ipv4(ip5, Some(28)).unwrap();
        assert_eq!(next_ip5, Ipv4::new("10.18.127.0/28").unwrap());

        let next_ip5 = next_subnet_ipv4(next_ip5, Some(24)).unwrap();
        assert_eq!(next_ip5, Ipv4::new("10.18.128.0/24").unwrap());
    }
    #[test]
    fn test_broadcast_addr() {
        let ip = Ipv4Addr::new(192, 168, 1, 0);
        assert_eq!(
            broadcast_addr(ip, 24).unwrap(),
            Ipv4Addr::new(192, 168, 1, 255)
        );
        assert_eq!(
            broadcast_addr(ip, 16).unwrap(),
            Ipv4Addr::new(192, 168, 255, 255)
        );
        assert_eq!(
            broadcast_addr(ip, 8).unwrap(),
            Ipv4Addr::new(192, 255, 255, 255)
        );
        assert_eq!(
            broadcast_addr(ip, 32).unwrap(),
            Ipv4Addr::new(192, 168, 1, 0)
        );

        assert!(broadcast_addr(Ipv4Addr::new(255, 255, 255, 255), 24).is_ok());
    }
    #[test]
    fn test_num_az_hosts() {
        assert_eq!(num_az_hosts(0).unwrap(), 4294967291); // 2^32 - 5
        assert_eq!(num_az_hosts(8).unwrap(), 16777211); // 2^24 - 5
        assert_eq!(num_az_hosts(16).unwrap(), 65531); // 2^16 - 5
        assert_eq!(num_az_hosts(24).unwrap(), 251); // 2^8 - 5
        assert_eq!(num_az_hosts(25).unwrap(), 123); // 2^7 - 5
        assert_eq!(num_az_hosts(26).unwrap(), 59); // 2^6 - 5
        assert_eq!(num_az_hosts(27).unwrap(), 27); // 2^5 - 5
        assert_eq!(num_az_hosts(28).unwrap(), 11); // 2^4 - 5
        assert_eq!(num_az_hosts(29).unwrap(), 3); // 2^3 - 5
        assert_eq!(
            num_az_hosts(30).unwrap_err().to_string(),
            "Network length is too long or invalid"
        );
        assert_eq!(
            num_az_hosts(32).unwrap_err().to_string(),
            "Network length is too long or invalid"
        );

        assert!(num_az_hosts(33).is_err());
    }

    #[test]
    fn test_ip4_cmp() {
        let ip1 = Ipv4::new("10.0.0.1/24").unwrap();
        let ip2 = Ipv4::new("10.0.0.2/24").unwrap();
        let ip3 = Ipv4::new("10.0.0.1/24").unwrap();

        assert!(ip1 < ip2);
        assert!(ip1 == ip3);
        assert!(ip2 > ip1);
        assert!(ip2 >= ip3);
    }
    #[test]
    fn test_ip4_cmp_overlap() {
        let ip1 = Ipv4::new("10.0.10.0/24").unwrap();
        let ip2 = Ipv4::new("10.0.0.0/8").unwrap();
        let ip3 = Ipv4::new("10.0.10.64/26").unwrap();

        assert!(ip1.addr > ip2.addr);
        assert!(ip1.addr < ip3.addr);
        assert!(ip1.mask > ip2.mask);
        // ip is start of subnet
        assert!(ip1 > ip2);
        assert!(ip1 < ip3);
        assert!(ip2 < ip1);
        assert!(ip2 < ip3);
        // min
        assert!(ip2.lo() < ip1.lo());
        assert!(ip2.lo() < ip3.lo());
        // max
        assert!(ip2.hi() > ip1.hi());
        assert!(ip2.hi() > ip3.hi());
        assert_eq!(ip2.hi(), Ipv4Addr::new(10, 255, 255, 255));
    }
    #[test]
    fn test_lo_mask() {
        let ip = Ipv4Addr::new(192, 168, 1, 1);
        assert_eq!(lo_mask(ip), 32);
    }
}
