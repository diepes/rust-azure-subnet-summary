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
A CSV row for a subnet belonging to an excluded VNet. `gap` column = `"DUP_EXCL_VNET"`. `subnet_name` = `"{original_subnet_name} [DUP of VNET {winner_vnet_name}]"`. All other fields are populated normally.

---

## VNet Peering

### Peering Edge
A directed connection from one VNet to a remote VNet. Azure requires both sides to be configured; a pair of Peering Edges (A→B and B→A) forms one logical connection. Each edge records its `peering_state` (`Connected`, `Disconnected`, `Initiated`) and a `remote_vnet_id` (ARM resource ID encoding the remote subscription and VNet name).
_Avoid_: peering, link, connection

### Subscription Island
A maximal set of VNets where every member can reach every other member via one or more Peering Edges. A VNet with no Peering Edges is an island of size one (standalone). Because VNets with overlapping CIDRs cannot be peered, each Conflict Group is always its own isolated Subscription Island.
_Avoid_: network island, VNet cluster, connected component

### Gateway VNet
A VNet that contains a subnet named exactly `GatewaySubnet`. That subnet hosts an Azure VPN Gateway or ExpressRoute Gateway, giving the VNet external connectivity to on-premises networks. Identified from existing subnet query data without any additional query. Shown with a distinct external node in the peering diagram.
_Avoid_: hub VNet (hub is a topology role, not a fixed property), gateway hub

### Missing VNet
A VNet referenced in a **Peering Edge** (as a remote target or source) that has no corresponding subnet records in the query results. Caused by VNets in subscriptions outside the query scope, deleted VNets with stale peering config, or cross-tenant VNets. Rendered with a dark-red node and cluster in the **Peering Diagram**, labelled `⚠ MISSING - SUB:<subscription>`.
_Avoid_: phantom VNet, ghost VNet, unknown VNet


A CSV row for the `GatewaySubnet` subnet inside a Gateway VNet. `gap` column = `"GATEWAY"`. All other fields are populated normally from the query result. Signals to downstream consumers that this VNet has external connectivity.

### Peering Diagram
A Mermaid diagram file (`subnets-YYYY-MM-DD-peering.md`) showing all VNets as labelled nodes. Each node label uses the format `SubscriptionName/VNetName` with the CIDR on a second line. VNets are grouped into `subgraph` blocks by Subscription Island. Gateway VNets have an additional external node attached to represent on-premises connectivity. Standalone VNets (no peerings) appear as single-node subgraphs.

Edge rendering rules:
- **Both sides `Connected`** → single bidirectional arrow (`A <--> B`)
- **Asymmetric or broken** (one side `Disconnected` or `Initiated`) → single directed arrow from the connected side with a stop/cross at the remote end (`A --x B`), styled red via `linkStyle`

---

## Key Invariants

- **Gap finder invariant**: All subnets passed to the gap finder must be in non-decreasing IP order with no CIDR overlaps. Violated if subnets from overlapping VNets are mixed.
- **Gap block boundary invariant**: A gap block must not cross a VNet CIDR boundary. Every gap row has exactly one label: `-vgap-` (inside a VNet) or `-gap-` (outside all VNets).
- **Excluded subnets stay in `Data`**: They are marked with `Subnet.excluded_by = Some(winner_vnet_name)` and skipped by the gap finder, but still emitted in the CSV.
- **No hardcoded exclusion list**: The old `filter_excluded_vnet_cidrs` / `default_vnet_cidrs_to_exclude` mechanism is removed. All conflicts are handled by generic overlap detection.
