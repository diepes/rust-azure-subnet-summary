//! Gap finding between subnets.
//!
//! Identifies unused IP address ranges between allocated subnets.

use crate::models::{next_subnet_ipv4, num_az_hosts, Ipv4, Subnet};
use std::net::Ipv4Addr;

// ─── VnetCidr + Gap Iterator ──────────────────────────────────────────────────

/// A VNet CIDR address space with its metadata and owned subnets.
///
/// Subnets are kept sorted by subnet start IP (smallest first). This encodes the
/// structural invariant that each subnet belongs to exactly one VNet CIDR.
#[derive(Debug)]
pub struct VnetCidr {
    pub cidr: Ipv4,
    pub vnet_name: String,
    pub subscription_id: String,
    pub subscription_name: String,
    pub location: String,
    /// Subnets belonging to this VNet CIDR, sorted by start IP.
    pub subnets: Vec<Subnet>,
}

/// A single block in the gap-scan output.
#[derive(Debug)]
pub struct GapEvent<'a> {
    /// The specific CIDR of this block.
    pub cidr: Ipv4,
    /// What this block represents.
    pub kind: GapKind<'a>,
}

/// Classifies a [`GapEvent`].
#[derive(Debug)]
pub enum GapKind<'a> {
    /// Unused address space between two VNet CIDRs.
    Gap,
    /// Unused address space inside a VNet CIDR (carries the VNet context).
    Vnet(&'a VnetCidr),
    /// An allocated subnet.
    Subnet(&'a Subnet),
}

/// Iterate over all blocks inside `vnet_cidrs` as a flat sequence of [`GapEvent`]s.
///
/// Each subnet becomes a `Subnet` event; unused space inside a VNet CIDR becomes
/// one or more `Vnet` events (split into aligned blocks up to `gap_mask`);
/// unused space between adjacent VNet CIDRs becomes one or more `Gap` events.
///
/// `vnet_cidrs` must be sorted by `cidr` (ascending) and each `VnetCidr`'s
/// `subnets` must be sorted by `subnet_cidr` (ascending).
pub fn gaps<'a>(vnet_cidrs: &'a [VnetCidr], gap_mask: u8) -> Vec<GapEvent<'a>> {
    let mut events = Vec::new();
    let mut current_ip: Option<Ipv4Addr> = None;

    for vc in vnet_cidrs {
        // Global gap before this VNet CIDR.
        if let Some(ip) = current_ip {
            let mut gip = ip;
            while gip < vc.cidr.lo() {
                let mask = find_biggest_subnet(gip, gap_mask, vc.cidr);
                let block = Ipv4 { addr: gip, mask };
                events.push(GapEvent {
                    cidr: block,
                    kind: GapKind::Gap,
                });
                gip = next_subnet_ipv4(block, None).unwrap().lo();
            }
        }

        // Subnets (and vgaps) inside this VNet CIDR.
        let mut inner_ip = vc.cidr.lo();
        for subnet in &vc.subnets {
            if let Some(sub_cidr) = subnet.subnet_cidr {
                // Vgap before this subnet.
                while inner_ip < sub_cidr.lo() {
                    let mask = find_biggest_subnet(inner_ip, gap_mask, sub_cidr);
                    let block = Ipv4 {
                        addr: inner_ip,
                        mask,
                    };
                    events.push(GapEvent {
                        cidr: block,
                        kind: GapKind::Vnet(vc),
                    });
                    inner_ip = next_subnet_ipv4(block, None).unwrap().lo();
                }
                events.push(GapEvent {
                    cidr: sub_cidr,
                    kind: GapKind::Subnet(subnet),
                });
                inner_ip = next_subnet_ipv4(sub_cidr, None).unwrap().lo();
            }
        }

        // Trailing vgap to end of VNet CIDR.
        while inner_ip <= vc.cidr.hi() {
            let mask = find_biggest_subnet_within(inner_ip, gap_mask, vc.cidr);
            let block = Ipv4 {
                addr: inner_ip,
                mask,
            };
            events.push(GapEvent {
                cidr: block,
                kind: GapKind::Vnet(vc),
            });
            inner_ip = next_subnet_ipv4(block, None).unwrap().lo();
        }

        current_ip = Some(next_subnet_ipv4(vc.cidr, None).unwrap().lo());
    }

    events
}

/// Context from the previous subnet's VNet, carried forward to identify gaps within VNets.
#[derive(Debug, Clone, Default)]
pub struct PrevVnetContext {
    pub vnet_cidr: Option<Ipv4>,
    pub vnet_name: String,
    pub subscription_name: String,
    pub subscription_id: String,
}

/// Represents a row of subnet data for output.
#[derive(Debug)]
pub struct SubnetPrintRow {
    /// Row index (0 for gap subnets).
    pub j: usize,
    /// Gap indicator or subnet source index.
    pub gap: String,
    /// Subnet CIDR notation.
    pub subnet_cidr: String,
    /// Broadcast address.
    pub broadcast: String,
    /// Number of usable Azure hosts.
    pub az_hosts: usize,
    /// Subnet name.
    pub subnet_name: String,
    /// Subscription display name.
    pub subscription_name: String,
    /// VNet CIDR notation.
    pub vnet_cidr: String,
    /// VNet name.
    pub vnet_name: String,
    /// Azure region.
    pub location: String,
    /// NSG name (extracted from full ID).
    pub nsg: String,
    /// DNS servers.
    pub dns: String,
    /// Subscription ID.
    pub subscription_id: String,
    /// Number of IP configurations using this subnet.
    pub ip_configurations_count: u32,
}

// ─── GapFinder ───────────────────────────────────────────────────────────────

/// Push-based accumulator that hides `next_ip` / `PrevVnetContext` state.
///
/// Feed sorted subnets one at a time with [`GapFinder::push`]; collect the
/// generated `SubnetPrintRow`s (including any gap rows) from each call.
/// After the last subnet call [`GapFinder::finish`] to get the trailing vgap
/// rows for the final VNet.
pub struct GapFinder {
    default_cidr_mask: u8,
    next_ip: Ipv4Addr,
    prev_vnet_ctx: PrevVnetContext,
}

impl GapFinder {
    /// Create a new `GapFinder`.
    ///
    /// * `default_cidr_mask` — maximum gap block size (e.g. `28` → `/28` blocks).
    pub fn new(default_cidr_mask: u8) -> Self {
        Self {
            default_cidr_mask,
            next_ip: Ipv4Addr::new(10, 0, 0, 0),
            prev_vnet_ctx: PrevVnetContext::default(),
        }
    }

    /// Process one subnet and return all rows it generates (gaps + the subnet itself).
    ///
    /// Subnets **must** be supplied in ascending CIDR order — the internal
    /// `assert` inside `process_subnet_row` will panic on out-of-order input.
    pub fn push(&mut self, s: &Subnet, i: usize) -> Vec<SubnetPrintRow> {
        const SKIP: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);
        let (new_next_ip, new_prev_ctx, rows) = process_subnet_row(
            s,
            i,
            self.next_ip,
            std::mem::take(&mut self.prev_vnet_ctx),
            self.default_cidr_mask,
            SKIP,
        );
        self.next_ip = new_next_ip;
        self.prev_vnet_ctx = new_prev_ctx;
        rows
    }

    /// Return trailing vgap rows for the last VNet seen, then reset state.
    ///
    /// Call once after all subnets have been pushed.  Safe to call even if no
    /// subnets were pushed (returns an empty `Vec`).
    pub fn finish(self) -> Vec<SubnetPrintRow> {
        match self.prev_vnet_ctx.vnet_cidr {
            None => vec![],
            Some(vnet_cidr) => {
                fill_trailing_vgap(
                    self.next_ip,
                    vnet_cidr,
                    &self.prev_vnet_ctx,
                    self.default_cidr_mask,
                )
                .1
            }
        }
    }
}

///
/// # Arguments
/// * `s` - The subnet to process
/// * `i` - The index of this subnet
/// * `next_ip` - The expected next IP address
/// * `prev_vnet_ctx` - Context from the previous VNet
/// * `default_cidr_mask` - Default mask size for gap subnets
/// * `_skip_subnet_smaller_than` - Skip subnets smaller than this (unused)
///
/// # Returns
/// A tuple of (next_ip, prev_vnet_ctx, rows)
#[allow(unused_variables)]
pub fn process_subnet_row(
    s: &Subnet,
    i: usize,
    mut next_ip: Ipv4Addr,
    prev_vnet_ctx: PrevVnetContext,
    default_cidr_mask: u8,
    _skip_subnet_smaller_than: Ipv4Addr,
) -> (Ipv4Addr, PrevVnetContext, Vec<SubnetPrintRow>) {
    let mut rows = Vec::new();

    // Handle empty subnet_cidr
    let subnet_cidr = match s.subnet_cidr {
        Some(s_cidr) => s_cidr,
        None => {
            log::warn!(
                "Warning: subnet_cidr is None for subnet_name: {}",
                s.subnet_name
            );
            rows.push(create_row_from_subnet(s, i, "None", "none", "none", 0));
            return (next_ip, prev_vnet_ctx, rows);
        }
    };

    // Look for unused subnet gaps
    assert!(
        next_ip <= subnet_cidr.addr,
        "next_ip[{next_ip}] > subnet_cidr[{subnet_cidr}] should never happen.\n  Subscription: '{subscription_name}',  Subnet: '{subnet_name}', Vnet_CIDR: '{vnet_cidr}'",
        subscription_name = s.subscription_name,
        subnet_name = s.subnet_name,
        vnet_cidr = s.vnet_cidr,
    );

    // Create gap subnets
    while next_ip < subnet_cidr.lo() {
        let next_mask = find_biggest_subnet(next_ip, default_cidr_mask, subnet_cidr);
        let next_subnet = Ipv4 {
            addr: next_ip,
            mask: next_mask,
        };

        // Check if gap is within the current or previous subnet's vnet
        let gap_in_current_vnet = s.vnet_cidr.contains(next_ip);
        let gap_in_prev_vnet = prev_vnet_ctx
            .vnet_cidr
            .is_some_and(|vnet| vnet.contains(next_ip));

        let (gap_label, gap_vnet_cidr, gap_vnet_name, gap_sub_name, gap_sub_id) =
            if gap_in_current_vnet {
                (
                    "-vgap-",
                    s.vnet_cidr.to_string(),
                    s.vnet_name.clone(),
                    s.subscription_name.clone(),
                    s.subscription_id.clone(),
                )
            } else if gap_in_prev_vnet {
                (
                    "-vgap-",
                    prev_vnet_ctx
                        .vnet_cidr
                        .map_or("None".to_string(), |v| v.to_string()),
                    prev_vnet_ctx.vnet_name.clone(),
                    prev_vnet_ctx.subscription_name.clone(),
                    prev_vnet_ctx.subscription_id.clone(),
                )
            } else {
                (
                    "-gap-",
                    "None".to_string(),
                    "None".to_string(),
                    "None".to_string(),
                    "None".to_string(),
                )
            };

        rows.push(SubnetPrintRow {
            j: 0,
            gap: gap_label.to_string(),
            subnet_cidr: next_subnet.to_string(),
            broadcast: next_subnet.broadcast().unwrap().addr.to_string(),
            az_hosts: num_az_hosts(next_mask).unwrap() as usize,
            subnet_name: "None".to_string(),
            subscription_name: gap_sub_name,
            vnet_cidr: gap_vnet_cidr,
            vnet_name: gap_vnet_name,
            location: "None".to_string(),
            nsg: "Unused_nsg".to_string(),
            dns: "Unused_dns".to_string(),
            subscription_id: gap_sub_id,
            ip_configurations_count: 0,
        });

        next_ip = next_subnet_ipv4(next_subnet, None).unwrap().lo();
    }

    let new_prev_vnet_ctx = PrevVnetContext {
        vnet_cidr: Some(s.vnet_cidr),
        vnet_name: s.vnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        subscription_id: s.subscription_id.clone(),
    };

    // Add the actual subnet row
    rows.push(SubnetPrintRow {
        j: i + 1,
        gap: if s.subnet_name == "GatewaySubnet" {
            "GATEWAY".to_string()
        } else {
            format!("Sub{}", i)
        },
        subnet_cidr: subnet_cidr.to_string(),
        broadcast: subnet_cidr.broadcast().unwrap().addr.to_string(),
        az_hosts: num_az_hosts(subnet_cidr.mask).unwrap() as usize,
        subnet_name: s.subnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        vnet_cidr: s.vnet_cidr.to_string(),
        vnet_name: s.vnet_name.clone(),
        location: s.location.clone(),
        nsg: extract_nsg_name(s.nsg.as_deref()),
        dns: format_dns_servers(s.dns_servers.as_deref()),
        subscription_id: s.subscription_id.clone(),
        ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
    });

    next_ip = next_subnet_ipv4(subnet_cidr, None).unwrap().lo();
    (next_ip, new_prev_vnet_ctx, rows)
}

/// Create a row for a subnet with no CIDR.
fn create_row_from_subnet(
    s: &Subnet,
    i: usize,
    gap: &str,
    cidr: &str,
    broadcast: &str,
    az_hosts: usize,
) -> SubnetPrintRow {
    SubnetPrintRow {
        j: i + 1,
        gap: gap.to_string(),
        subnet_cidr: cidr.to_string(),
        broadcast: broadcast.to_string(),
        az_hosts,
        subnet_name: s.subnet_name.clone(),
        subscription_name: s.subscription_name.clone(),
        vnet_cidr: s.vnet_cidr.to_string(),
        vnet_name: s.vnet_name.clone(),
        location: s.location.clone(),
        nsg: extract_nsg_name(s.nsg.as_deref()),
        dns: format_dns_servers(s.dns_servers.as_deref()),
        subscription_id: s.subscription_id.clone(),
        ip_configurations_count: s.ip_configurations_count.unwrap_or(0),
    }
}

/// Extract NSG name from full resource ID.
pub(crate) fn extract_nsg_name(nsg: Option<&str>) -> String {
    nsg.unwrap_or("None")
        .split('/')
        .next_back()
        .unwrap_or("None")
        .to_string()
}

/// Format DNS servers as a comma-separated string.
pub(crate) fn format_dns_servers(dns: Option<&[String]>) -> String {
    dns.map(|servers| servers.join(","))
        .unwrap_or_else(|| "None".to_string())
}

/// Fill the trailing unused space within a VNet_CIDR as `-vgap-` rows.
///
/// Called after all subnets in the dataset (or after the last subnet in a VNet_CIDR group)
/// to fill address space from `next_ip` to the VNet_CIDR's broadcast.
///
/// Returns `(new_next_ip, rows)`.
pub fn fill_trailing_vgap(
    mut next_ip: Ipv4Addr,
    vnet_cidr: Ipv4,
    prev_vnet_ctx: &PrevVnetContext,
    default_cidr_mask: u8,
) -> (Ipv4Addr, Vec<SubnetPrintRow>) {
    let mut rows = Vec::new();
    let vnet_hi = vnet_cidr.hi();

    while next_ip <= vnet_hi {
        let next_mask = find_biggest_subnet_within(next_ip, default_cidr_mask, vnet_cidr);
        let next_subnet = Ipv4 {
            addr: next_ip,
            mask: next_mask,
        };

        rows.push(SubnetPrintRow {
            j: 0,
            gap: "-vgap-".to_string(),
            subnet_cidr: next_subnet.to_string(),
            broadcast: next_subnet.broadcast().unwrap().addr.to_string(),
            az_hosts: num_az_hosts(next_mask).unwrap() as usize,
            subnet_name: "None".to_string(),
            subscription_name: prev_vnet_ctx.subscription_name.clone(),
            vnet_cidr: vnet_cidr.to_string(),
            vnet_name: prev_vnet_ctx.vnet_name.clone(),
            location: "None".to_string(),
            nsg: "Unused_nsg".to_string(),
            dns: "Unused_dns".to_string(),
            subscription_id: prev_vnet_ctx.subscription_id.clone(),
            ip_configurations_count: 0,
        });

        next_ip = next_subnet_ipv4(next_subnet, None).unwrap().lo();
    }

    (next_ip, rows)
}

/// Find the biggest subnet starting at `start_ip` that fits entirely within `vnet_cidr`.
fn find_biggest_subnet_within(start_ip: Ipv4Addr, start_mask: u8, vnet_cidr: Ipv4) -> u8 {
    let min_mask_for_alignment = crate::models::lo_mask(start_ip);
    let mut next_mask = start_mask.max(min_mask_for_alignment);

    loop {
        let next_subnet = Ipv4 {
            addr: start_ip,
            mask: next_mask,
        };
        if next_subnet.hi() > vnet_cidr.hi() {
            next_mask += 1;
        } else {
            break;
        }
    }

    assert!(
        next_mask <= 32,
        "next_mask[{next_mask}] > 32 should never happen."
    );
    next_mask
}

/// Find the biggest subnet that fits before the target subnet.
///
/// The returned mask is constrained by:
/// 1. The `start_mask` parameter (won't return a smaller mask)
/// 2. The IP alignment - `start_ip` must be a valid network address for the mask
/// 3. The subnet must not overlap with `below_subnet_cidr`
fn find_biggest_subnet(start_ip: Ipv4Addr, start_mask: u8, below_subnet_cidr: Ipv4) -> u8 {
    assert!(
        start_mask <= 32,
        "start_mask[{start_mask}] > 32 should never happen."
    );

    // Calculate minimum valid mask based on IP alignment (trailing zeros)
    let min_mask_for_alignment = crate::models::lo_mask(start_ip);

    // Start with the larger (more restrictive) of start_mask and alignment requirement
    let mut next_mask = start_mask.max(min_mask_for_alignment);

    loop {
        let next_subnet = Ipv4 {
            addr: start_ip,
            mask: next_mask,
        };
        if next_subnet.hi() >= below_subnet_cidr.lo() {
            next_mask += 1;
        } else {
            break;
        }
    }

    assert!(
        next_mask <= 32,
        "next_mask[{next_mask}] > 32 should never happen."
    );
    next_mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_biggest_subnet() {
        // 10.0.0.0 is aligned to any mask (trailing zeros = 24 bits in last 3 octets)
        let start_ip = Ipv4Addr::new(10, 0, 0, 0);
        let below_subnet_cidr = Ipv4::new("10.0.1.0/24").unwrap();
        assert_eq!(24, find_biggest_subnet(start_ip, 8, below_subnet_cidr));
        assert_eq!(28, find_biggest_subnet(start_ip, 28, below_subnet_cidr));

        // 10.11.12.16 has 4 trailing zeros, so min mask = 28
        // Even though we ask for start_mask=8, alignment constrains to /28
        let start_ip = Ipv4Addr::new(10, 11, 12, 16);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(28, find_biggest_subnet(start_ip, 8, below_subnet_cidr));

        // 10.11.12.0 has 10 trailing zeros (12 = 0b00001100, ends in 00), min mask = 22
        // So it can be a valid /22 network address
        let start_ip = Ipv4Addr::new(10, 11, 12, 0);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(22, find_biggest_subnet(start_ip, 8, below_subnet_cidr));

        let start_ip = Ipv4Addr::new(10, 0, 0, 0);
        let below_subnet_cidr = Ipv4::new("10.11.16.0/24").unwrap();
        assert_eq!(13, find_biggest_subnet(start_ip, 8, below_subnet_cidr));

        let below_subnet_cidr = Ipv4::new("10.192.0.0/24").unwrap();
        assert_eq!(9, find_biggest_subnet(start_ip, 8, below_subnet_cidr));
        assert_eq!(12, find_biggest_subnet(start_ip, 12, below_subnet_cidr));
    }

    #[test]
    fn test_find_biggest_subnet_alignment() {
        // Test the bug fix: 10.6.2.80 can only be /28 or smaller due to alignment
        // 10.6.2.80 binary ends in 0101_0000, so trailing zeros = 4, lo_mask = 28
        let start_ip = Ipv4Addr::new(10, 6, 2, 80);
        let below_subnet_cidr = Ipv4::new("10.6.8.0/24").unwrap();

        // Without the fix, this would return /21 which is invalid for 10.6.2.80
        // With the fix, it should return /28 (constrained by IP alignment)
        let mask = find_biggest_subnet(start_ip, 16, below_subnet_cidr);
        assert_eq!(
            28, mask,
            "10.6.2.80 can only be /28 or smaller due to alignment"
        );

        // Verify the resulting subnet is valid
        let gap_subnet = Ipv4::new("10.6.2.80/28").unwrap();
        assert_eq!(
            gap_subnet.lo(),
            start_ip,
            "Network address should match start_ip"
        );
        assert!(
            gap_subnet.hi() < below_subnet_cidr.lo(),
            "Gap should not overlap with next subnet"
        );
    }
    // Helper to build a minimal Subnet for gap tests.
    fn make_subnet(cidr: &str, vnet_cidr: &str, vnet_name: &str, subnet_name: &str) -> Subnet {
        let mut s: Subnet = Default::default();
        s.vnet_name = vnet_name.to_string();
        s.vnet_cidr = Ipv4::new(vnet_cidr).unwrap();
        s.subnet_name = subnet_name.to_string();
        s.subnet_cidr = Some(Ipv4::new(cidr).unwrap());
        s
    }

    const SKIP: Ipv4Addr = Ipv4Addr::new(10, 17, 255, 255);

    /// With mask=4 a five-/16 gap collapses to 2 rows; mask=16 produces 5.
    /// This is the primary regression guard for the DEFAULT_CIDR_MASK=4 change.
    #[test]
    fn large_gap_with_mask_4_produces_fewer_rows_than_mask_16() {
        let s = make_subnet("10.5.0.0/24", "10.5.0.0/16", "vnet-b", "snet-b");
        let start = Ipv4Addr::new(10, 0, 0, 0);

        let (_, _, rows_4) = process_subnet_row(&s, 0, start, PrevVnetContext::default(), 4, SKIP);
        let (_, _, rows_16) =
            process_subnet_row(&s, 0, start, PrevVnetContext::default(), 16, SKIP);

        let gaps_4: Vec<_> = rows_4.iter().filter(|r| r.j == 0).collect();
        let gaps_16: Vec<_> = rows_16.iter().filter(|r| r.j == 0).collect();

        assert_eq!(gaps_16.len(), 5, "mask=16: one /16 row per class-B");
        assert_eq!(gaps_4.len(), 2, "mask=4: 10.0.0.0/14 + 10.4.0.0/16");
        assert_eq!(gaps_4[0].subnet_cidr, "10.0.0.0/14");
        assert_eq!(gaps_4[1].subnet_cidr, "10.4.0.0/16");
    }

    /// Gap blocks that start inside a VNet must not cross the VNet's broadcast.
    /// Alignment naturally enforces this; this test guards against regression.
    #[test]
    fn gap_inside_vnet_stays_within_vnet_boundary() {
        // First subnet in vnet-a is at 10.0.64.0/24; gap fills 10.0.0.0..10.0.63.255.
        let s = make_subnet("10.0.64.0/24", "10.0.0.0/16", "vnet-a", "snet-a");
        let vnet_hi = Ipv4::new("10.0.0.0/16").unwrap().hi();

        let (_, _, rows) = process_subnet_row(
            &s,
            0,
            Ipv4Addr::new(10, 0, 0, 0),
            PrevVnetContext::default(),
            4,
            SKIP,
        );

        for row in rows.iter().filter(|r| r.j == 0) {
            let gap = Ipv4::new(&row.subnet_cidr).unwrap();
            assert!(
                gap.hi() <= vnet_hi,
                "Gap {} crosses VNet boundary (hi={}, vnet_hi={})",
                row.subnet_cidr,
                gap.hi(),
                vnet_hi,
            );
        }
    }

    #[test]
    fn gateway_subnet_gets_gateway_gap_marker() {
        let s = make_subnet("10.0.0.0/27", "10.0.0.0/16", "hub-vnet", "GatewaySubnet");
        let (_, _, rows) = process_subnet_row(
            &s,
            0,
            Ipv4Addr::new(10, 0, 0, 0),
            PrevVnetContext::default(),
            28,
            SKIP,
        );
        let subnet_row = rows
            .iter()
            .find(|r| r.j != 0)
            .expect("should have a subnet row");
        assert_eq!(
            subnet_row.gap, "GATEWAY",
            "GatewaySubnet must have gap = GATEWAY"
        );
    }

    #[test]
    fn test_process_subnet_row_01() {
        let mut result: Subnet = Default::default();
        result.vnet_name = "jenkinsarm-vnet".to_string();
        result.vnet_cidr = Ipv4::new("10.0.0.0/16").unwrap();
        result.subnet_name = "jenkinsarm-snet".to_string();
        result.subnet_cidr = Some(Ipv4::new("10.0.0.0/24").unwrap());

        let (next_ip, _prev_vnet_ctx, print_rows) = process_subnet_row(
            &result,
            1,
            Ipv4Addr::new(10, 0, 0, 0),
            PrevVnetContext::default(),
            28,
            Ipv4Addr::new(10, 17, 255, 255),
        );

        assert_eq!(result.subnet_name, "jenkinsarm-snet");
        assert_eq!(next_ip.to_string(), "10.0.1.0");
        assert_eq!(print_rows.len(), 1, "Expected 1 row for subnet");
    }

    // ─── gaps() tests ──────────────────────────────────────────────────────────

    fn make_vnet_cidr(cidr: &str, name: &str, subnets: Vec<Subnet>) -> VnetCidr {
        VnetCidr {
            cidr: Ipv4::new(cidr).unwrap(),
            vnet_name: name.to_string(),
            subscription_id: "sub-001".to_string(),
            subscription_name: "Test Sub".to_string(),
            location: "eastus".to_string(),
            subnets,
        }
    }

    #[test]
    fn single_subnet_filling_vnet_cidr_produces_one_subnet_event() {
        let subnet = make_subnet("10.0.0.0/24", "10.0.0.0/24", "vnet-a", "snet-a");
        let vc = make_vnet_cidr("10.0.0.0/24", "vnet-a", vec![subnet]);
        let vnet_cidrs = [vc];
        let events = gaps(&vnet_cidrs, 28);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].kind, GapKind::Subnet(_)));
        assert_eq!(events[0].cidr.to_string(), "10.0.0.0/24");
    }

    #[test]
    fn vgap_before_first_subnet_is_emitted_as_vnet_event() {
        // 10.0.0.0/16 VNet, first subnet starts at 10.0.1.0/24 — gap before it
        let subnet = make_subnet("10.0.1.0/24", "10.0.0.0/16", "vnet-a", "snet-a");
        let vc = make_vnet_cidr("10.0.0.0/16", "vnet-a", vec![subnet]);
        let vnet_cidrs = [vc];
        let events = gaps(&vnet_cidrs, 24);

        // first event must be a Vnet gap covering 10.0.0.0/24
        assert!(matches!(events[0].kind, GapKind::Vnet(_)));
        assert_eq!(events[0].cidr.to_string(), "10.0.0.0/24");
        // second event is the real subnet
        assert!(matches!(events[1].kind, GapKind::Subnet(_)));
    }

    #[test]
    fn vgap_between_two_subnets_is_emitted_as_vnet_event() {
        let s1 = make_subnet("10.0.0.0/24", "10.0.0.0/16", "vnet-a", "snet-a");
        let s2 = make_subnet("10.0.2.0/24", "10.0.0.0/16", "vnet-a", "snet-b");
        let vc = make_vnet_cidr("10.0.0.0/16", "vnet-a", vec![s1, s2]);
        let vnet_cidrs = [vc];
        let events = gaps(&vnet_cidrs, 24);

        // snet-a, 10.0.1.0/24 vgap, snet-b, trailing vgaps
        assert!(
            matches!(events[0].kind, GapKind::Subnet(_)),
            "first event should be snet-a"
        );
        assert_eq!(events[0].cidr.to_string(), "10.0.0.0/24");
        assert!(
            matches!(events[1].kind, GapKind::Vnet(_)),
            "second event should be vgap"
        );
        assert_eq!(events[1].cidr.to_string(), "10.0.1.0/24");
        assert!(
            matches!(events[2].kind, GapKind::Subnet(_)),
            "third event should be snet-b"
        );
    }

    #[test]
    fn trailing_vgap_fills_rest_of_vnet_cidr() {
        // subnet fills only /24 of a /16 — trailing space should emit Vnet events
        let subnet = make_subnet("10.0.0.0/24", "10.0.0.0/16", "vnet-a", "snet-a");
        let vc = make_vnet_cidr("10.0.0.0/16", "vnet-a", vec![subnet]);
        let vnet_cidrs = [vc];
        let events = gaps(&vnet_cidrs, 24);

        // first event is the subnet, rest are vgaps
        assert!(matches!(events[0].kind, GapKind::Subnet(_)));
        assert!(events[1..]
            .iter()
            .all(|e| matches!(e.kind, GapKind::Vnet(_))));
        // trailing vgaps must cover up to end of 10.0.0.0/16
        let last = events.last().unwrap();
        assert_eq!(last.cidr.hi(), Ipv4::new("10.0.0.0/16").unwrap().hi());
    }

    #[test]
    fn gap_between_two_vnet_cidrs_emits_gap_events() {
        // Two /24 VNets separated by a /24 hole: 10.0.0.0/24, hole 10.0.1.0/24, 10.0.2.0/24
        let s1 = make_subnet("10.0.0.0/24", "10.0.0.0/24", "vnet-a", "snet-a");
        let s2 = make_subnet("10.0.2.0/24", "10.0.2.0/24", "vnet-b", "snet-b");
        let vc1 = make_vnet_cidr("10.0.0.0/24", "vnet-a", vec![s1]);
        let vc2 = make_vnet_cidr("10.0.2.0/24", "vnet-b", vec![s2]);
        let vnet_cidrs = [vc1, vc2];
        let events = gaps(&vnet_cidrs, 24);

        // snet-a, Gap(10.0.1.0/24), snet-b
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0].kind, GapKind::Subnet(_)));
        assert!(
            matches!(events[1].kind, GapKind::Gap),
            "middle block should be a Gap"
        );
        assert_eq!(events[1].cidr.to_string(), "10.0.1.0/24");
        assert!(matches!(events[2].kind, GapKind::Subnet(_)));
    }

    #[test]
    fn large_gap_is_split_into_aligned_blocks_according_to_gap_mask() {
        // Five-/16 global gap: 10.0.0.0/24 VNet then 10.5.0.0/24 VNet, gap_mask=16
        // gap covers 10.0.1.0–10.4.255.255 → should produce multiple blocks, all Gap kind
        let s1 = make_subnet("10.0.0.0/24", "10.0.0.0/24", "vnet-a", "snet-a");
        let s2 = make_subnet("10.5.0.0/24", "10.5.0.0/24", "vnet-b", "snet-b");
        let vc1 = make_vnet_cidr("10.0.0.0/24", "vnet-a", vec![s1]);
        let vc2 = make_vnet_cidr("10.5.0.0/24", "vnet-b", vec![s2]);
        let vnet_cidrs = [vc1, vc2];

        let events_m16 = gaps(&vnet_cidrs, 16);
        let events_m4 = gaps(&vnet_cidrs, 4);

        let gap_count_m16 = events_m16
            .iter()
            .filter(|e| matches!(e.kind, GapKind::Gap))
            .count();
        let gap_count_m4 = events_m4
            .iter()
            .filter(|e| matches!(e.kind, GapKind::Gap))
            .count();

        assert!(
            gap_count_m16 > gap_count_m4,
            "coarser gap_mask should produce fewer blocks"
        );
        assert!(events_m16
            .iter()
            .filter(|e| matches!(e.kind, GapKind::Gap))
            .all(|e| e.cidr.mask >= 16));
    }

    /// RED: GapFinder owns state; push() returns rows for each subnet.
    #[test]
    fn gap_finder_push_accumulates_rows_and_hides_state() {
        // mask=24 → the one /24 gap between snet-a and snet-b becomes a single row
        let mut gf = GapFinder::new(24);
        let s1 = make_subnet("10.0.0.0/24", "10.0.0.0/16", "vnet-a", "snet-a");
        let s2 = make_subnet("10.0.2.0/24", "10.0.0.0/16", "vnet-a", "snet-b");

        let rows1 = gf.push(&s1, 0);
        let rows2 = gf.push(&s2, 1);
        let trailing = gf.finish();

        // s1 has no gap before it — just the one subnet row
        assert_eq!(rows1.len(), 1, "s1: no gap expected");
        assert_eq!(rows1[0].subnet_name, "snet-a");

        // s2: 10.0.1.0/24 gap then snet-b → 2 rows
        assert_eq!(rows2.len(), 2, "s2: one gap + one subnet");
        assert_eq!(rows2[0].gap, "-vgap-");
        assert_eq!(rows2[0].subnet_cidr, "10.0.1.0/24");
        assert_eq!(rows2[1].subnet_name, "snet-b");

        // trailing vgap fills rest of 10.0.0.0/16
        assert!(!trailing.is_empty(), "trailing vgaps expected");
        assert!(trailing.iter().all(|r| r.gap == "-vgap-"));
    }
}
