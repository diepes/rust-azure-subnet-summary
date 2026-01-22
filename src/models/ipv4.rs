//! IPv4 address and CIDR notation utilities.
//!
//! Provides [`Ipv4`] struct for representing IPv4 addresses with subnet masks,
//! along with utility functions for subnet calculations.

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use std::error::Error;
use std::net::Ipv4Addr;
use std::str::FromStr;

/// Maximum length for an IPv4 subnet mask (32 bits).
pub const MAX_LENGTH: u8 = 32;

/// Get the CIDR mask as a u32 from an [`Ipv4`] struct.
pub fn get_cidr_mask_ipv4(ipv4: Ipv4) -> Result<u32, Box<dyn Error>> {
    get_cidr_mask(ipv4.mask)
}

/// Convert a CIDR prefix length to a subnet mask as u32.
///
/// # Examples
/// ```
/// use azure_subnet_summary::models::get_cidr_mask;
/// assert_eq!(get_cidr_mask(24).unwrap(), 0xFFFFFF00);
/// ```
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

/// Cut an [`Ipv4`] address to a smaller subnet size.
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

/// Get the network address for a given IP and prefix length.
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

/// Calculate the next subnet after the given [`Ipv4`] subnet.
///
/// If `mask` is provided, the next subnet will use that mask size.
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
        // smaller subnet
        let current_broadcast = broadcast_addr(ipv4.addr, current_mask)?;
        let next_subnet = ip_after_subnet(current_broadcast, new_mask)?;
        Ok(Ipv4 {
            addr: next_subnet,
            mask: new_mask,
        })
    }
}

/// Returns the IP address following the given subnet.
pub fn ip_after_subnet(addr: Ipv4Addr, cidr: u8) -> Result<Ipv4Addr, Box<dyn Error>> {
    if cidr > MAX_LENGTH {
        Err("Network length is too long".into())
    } else {
        let subnet_size = 1 << (MAX_LENGTH - cidr);
        let addr_bits = u32::from(addr);
        let network_bits = addr_bits & get_cidr_mask(cidr)?;
        let next_subnet_bits = network_bits
            .checked_add(subnet_size)
            .ok_or("Next subnet calculation overflowed")?;
        Ok(Ipv4Addr::from(next_subnet_bits))
    }
}

/// Calculate the broadcast address for a given IP and prefix length.
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

/// Calculate the number of usable host addresses in an Azure subnet.
///
/// Azure reserves 5 IP addresses per subnet (network, broadcast, gateway, and 2 DNS).
pub fn num_az_hosts(len: u8) -> Result<u64, Box<dyn Error>> {
    if len >= MAX_LENGTH - 2 {
        // /29 = 6 IPs, only 1 host usable
        Err("Network length is too long or invalid".into())
    } else {
        let num_az_hosts = (1u64 << (MAX_LENGTH - len)) - 5;
        Ok(num_az_hosts)
    }
}

/// Calculate the minimum mask for an IP address based on trailing zeros.
pub fn lo_mask(ip: Ipv4Addr) -> u8 {
    let ip_u32 = u32::from(ip);
    let trailing_zeros = ip_u32.trailing_zeros() as u8;
    assert!(trailing_zeros <= 32, "Trailing zeros exceed 32 bits");
    32 - trailing_zeros
}

/// IPv4 address with CIDR notation support.
#[derive(Eq, Ord, Debug, Copy, Clone, Hash)]
pub struct Ipv4 {
    /// The IPv4 address.
    pub addr: Ipv4Addr,
    /// The subnet mask length (0-32).
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
            return Err(de::Error::custom(format!("invalid CIDR format: {}", s)));
        }

        let addr = Ipv4Addr::from_str(parts[0])
            .map_err(|_| de::Error::custom(format!("invalid IP address: {}", parts[0])))?;
        let mask = u8::from_str(parts[1])
            .map_err(|_| de::Error::custom(format!("invalid subnet mask: {}", parts[1])))?;

        Ok(Ipv4 { addr, mask })
    }
}

impl Ipv4 {
    /// Create a new [`Ipv4`] from a CIDR string (e.g., "10.0.0.0/24").
    pub fn new(addr_cidr: &str) -> Result<Ipv4, Box<dyn Error>> {
        let addr_cidr = addr_cidr.trim();
        let parts: Vec<&str> = addr_cidr.split('/').collect();
        if parts.len() != 2 {
            return Err("Invalid address/mask".into());
        }
        let addr: Ipv4Addr = parts[0]
            .parse()
            .map_err(|_| format!("Invalid address {}", parts[0]))?;
        let mask: u8 = parts[1].parse()?;
        if mask > MAX_LENGTH {
            return Err("Network length is too long".into());
        }
        Ok(Ipv4 { addr, mask })
    }

    /// Get the broadcast address for this subnet.
    pub fn broadcast(&self) -> Result<Ipv4, Box<dyn Error>> {
        let broadcast = broadcast_addr(self.addr, self.mask)?;
        Ok(Ipv4 {
            addr: broadcast,
            mask: self.mask,
        })
    }

    /// Get the highest (broadcast) address in the subnet.
    pub fn hi(&self) -> Ipv4Addr {
        broadcast_addr(self.addr, self.mask)
            .unwrap_or_else(|e| panic!("Error calculating broadcast address: {}", e))
    }

    /// Get the lowest (network) address in the subnet.
    pub fn lo(&self) -> Ipv4Addr {
        cut_addr(self.addr, self.mask)
            .unwrap_or_else(|e| panic!("Error calculating minimum address for {}: {}", self, e))
    }
}

impl std::fmt::Display for Ipv4 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.mask)
    }
}

impl PartialEq for Ipv4 {
    fn eq(&self, other: &Ipv4) -> bool {
        self.addr == other.addr && self.mask == other.mask
    }
}

impl PartialOrd for Ipv4 {
    fn partial_cmp(&self, other: &Ipv4) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
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

        let ipv4 = Ipv4::new("192.168.1.0/8").unwrap();
        assert_eq!(
            ipv4.broadcast().unwrap(),
            Ipv4::new("192.255.255.255/8").unwrap()
        );
        assert_eq!(
            next_subnet_ipv4(ipv4, None).unwrap(),
            Ipv4::new("193.0.0.0/8").unwrap()
        );

        let next_ipv4 = next_subnet_ipv4(ipv4, Some(16)).unwrap();
        assert_eq!(next_ipv4.mask, 16);
        assert_eq!(next_ipv4.addr, Ipv4Addr::new(193, 0, 0, 0));

        let ip3 = Ipv4::new("10.2.3.4/16").unwrap();
        assert_eq!(
            next_subnet_ipv4(ip3, None).unwrap(),
            Ipv4::new("10.3.0.0/16").unwrap()
        );
        assert_eq!(
            next_subnet_ipv4(ip3, Some(24)).unwrap(),
            Ipv4::new("10.3.0.0/24").unwrap()
        );

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
        assert_eq!(num_az_hosts(0).unwrap(), 4294967291);
        assert_eq!(num_az_hosts(8).unwrap(), 16777211);
        assert_eq!(num_az_hosts(16).unwrap(), 65531);
        assert_eq!(num_az_hosts(24).unwrap(), 251);
        assert_eq!(num_az_hosts(25).unwrap(), 123);
        assert_eq!(num_az_hosts(26).unwrap(), 59);
        assert_eq!(num_az_hosts(27).unwrap(), 27);
        assert_eq!(num_az_hosts(28).unwrap(), 11);
        assert_eq!(num_az_hosts(29).unwrap(), 3);
        assert_eq!(
            num_az_hosts(30).unwrap_err().to_string(),
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
        assert!(ip1 > ip2);
        assert!(ip1 < ip3);
        assert!(ip2 < ip1);
        assert!(ip2 < ip3);
        assert!(ip2.lo() < ip1.lo());
        assert!(ip2.lo() < ip3.lo());
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
