# Azure Subnet Summary — Context

## Purpose

Command-line tool that queries Azure Resource Graph for all subnets across subscriptions, detects unused IP address ranges (gaps), and outputs a CSV for capacity planning.

---

## Glossary

### VNet (Virtual Network)
An Azure Virtual Network. Has one or more **VNet CIDRs** and contains **Subnets**.

### VNet CIDR
The address space declared for a VNet (e.g., `10.11.0.0/16`). A single VNet may have multiple CIDRs.

### Subnet
An IP range within a VNet (e.g., `10.11.4.0/22`). Each subnet belongs to exactly one VNet and one subscription.

### Subscription
An Azure billing/organisational boundary. A VNet belongs to one subscription. Subscription names are used to classify VNets as **production** or non-production.

### Production Subscription
A subscription whose name contains `"prod"` (case-insensitive substring match). Examples: `"Coretex Production"`, `"iBright Production"`, `"platform-prod"`. Used in **Conflict Resolution**.

### Gap
Unused IP address space between two consecutive subnets (across the full range scanned). Emitted as synthetic rows in the CSV with `gap = "-gap-"` (outside any VNet) or `"-vgap-"` (inside a VNet's CIDR but unallocated). Each gap row is at most one **Gap Block**.

### Gap Block
A single synthetic gap row. Its CIDR is chosen to be as large as possible (smallest `/x` mask) subject to three constraints: (1) IP alignment (the block must be a valid network address for its mask), (2) it must end strictly before the next real subnet, and (3) it must not cross a **VNet CIDR boundary** — if a gap starts inside a VNet it stops at the VNet's broadcast; if a gap is about to enter a VNet it stops at that VNet's first address. The minimum allowed mask is controlled by `--gap-mask` (default `4`).

### VNet CIDR Boundary
The boundary between IP space inside a VNet CIDR and IP space outside it. Gap blocks are split at VNet CIDR boundaries so that each gap row has an unambiguous `-vgap-` or `-gap-` label.

### Overlap
Two VNets **overlap** when their CIDR ranges intersect: `A.lo() <= B.hi() && B.lo() <= A.hi()`. This includes exact-match (`10.0.0.0/16` vs `10.0.0.0/16`), containment (`10.0.0.0/8` contains `10.1.0.0/16`), and partial overlap.

### Conflict Group
A set of VNets that are transitively overlapping. If A overlaps B and B overlaps C, then {A, B, C} form one conflict group even if A and C do not directly overlap. Exactly one VNet per conflict group is **kept**; the rest are **excluded**.

### Conflict Resolution
Priority order for selecting the kept VNet within a conflict group:
1. **Production subscription** — VNet whose subscription name contains `"prod"` (case-insensitive) wins over non-production.
2. **Most subnets** — more subnets indicates more active use.
3. **Alphabetical** — by subscription name, ascending.

### Excluded VNet
A VNet that lost conflict resolution. Its subnets are not used in gap calculation. They are emitted in the CSV as `DUP_EXCL_VNET` rows and shown in terminal output in red, grouped with their conflict group.

### DUP_EXCL_VNET row
A CSV row for a subnet belonging to an excluded VNet. `gap` column = `"DUP_EXCL_VNET"`. `subnet_name` = `"{original_subnet_name} [DUP of {winner_vnet_name}]"`. All other fields are populated normally.

---

## Key Invariants

- **Gap finder invariant**: All subnets passed to the gap finder must be in non-decreasing IP order with no CIDR overlaps. Violated if subnets from overlapping VNets are mixed.
- **Gap block boundary invariant**: A gap block must not cross a VNet CIDR boundary. Every gap row has exactly one label: `-vgap-` (inside a VNet) or `-gap-` (outside all VNets).
- **Excluded subnets stay in `Data`**: They are marked with `Subnet.excluded_by = Some(winner_vnet_name)` and skipped by the gap finder, but still emitted in the CSV.
- **No hardcoded exclusion list**: The old `filter_excluded_vnet_cidrs` / `default_vnet_cidrs_to_exclude` mechanism is removed. All conflicts are handled by generic overlap detection.
