# Azure Subnet Summary — Context

## Purpose

Command-line tool that queries Azure Resource Graph for all subnets across subscriptions, detects unused IP address ranges (gaps), and outputs a CSV for capacity planning.

---

## Glossary

### VNet (Virtual Network)
An Azure Virtual Network. Stores a name, the owning **Subscription**, one or more **VNet_CIDRs**, DNS server IPs, and **Peering Edges**.

### VNet_CIDR
A single IP address block declared on a VNet (e.g., `10.11.0.0/16`). This is the atomic unit for IP-space reservation, overlap detection, and gap-finding. A **Subnet** belongs to exactly one VNet_CIDR.

### VNet_CIDRs
The ordered list of all **VNet_CIDR** entries on a VNet. A VNet may have one or more VNet_CIDRs; most have exactly one.

### Subnet
An IP range within a VNet (e.g., `10.11.4.0/22`). Each subnet belongs to exactly one **VNet_CIDR** (and therefore exactly one VNet and one Subscription). A Subnet is self-describing: the triple `(vnet_name, subscription_id, vnet_cidr)` is the composite key that uniquely identifies its containing `(VNet, VNet_CIDR)` pair.

**Flat model**: `Subnet` stores its VNet_CIDR as a plain `Ipv4` value — the value acts as the reference key. Code that needs to group by VNet_CIDR builds temporary collections keyed on this triple; there is no pointer or index back to a parent object.

### Subscription
An Azure billing/organisational boundary. A VNet belongs to one subscription. Subscription names are used to classify VNets as **production** or non-production.

### Production Subscription
A subscription whose name contains `"prod"` (case-insensitive substring match). Examples: `"Coretex Production"`, `"iBright Production"`, `"platform-prod"`. Used in **Conflict Resolution**.

### Gap
Unused IP address space not covered by a subnet. Emitted as synthetic rows in the CSV with `gap = "-gap-"` (outside any VNet_CIDR) or `"-vgap-"` (inside a VNet_CIDR but unallocated). Each gap row is at most one **Gap Block**.

### Gap Block
A single synthetic gap row. Its CIDR is chosen to be as large as possible (smallest `/x` mask) subject to three constraints: (1) IP alignment (the block must be a valid network address for its mask), (2) it must end strictly before the next real subnet or VNet_CIDR boundary, and (3) it must not cross a **VNet_CIDR Boundary**. The minimum allowed mask is controlled by `--gap-mask` (default `4`).

### Gap-Finding Loop
The CSV generator walks VNet_CIDRs in start-IP order. For each VNet_CIDR:
1. Emit global `-gap-` rows from the previous end up to the VNet_CIDR's first address.
2. Walk the VNet_CIDR's subnets in IP order, emitting `-vgap-` rows in the gaps between them.
3. After the last subnet, emit trailing `-vgap-` rows to fill the remainder of the VNet_CIDR up to its broadcast address.

After all VNet_CIDRs, emit any trailing global `-gap-` rows.

### Overlap
Two **VNet_CIDRs** from **different VNets** overlap when their IP ranges intersect: `A.lo() <= B.hi() && B.lo() <= A.hi()`. This includes exact-match, containment, and partial overlap. Two VNet_CIDRs on the **same VNet** never form a conflict regardless of their IP ranges.

### Conflict Group
A set of **VNet_CIDRs** (from different VNets) that are transitively overlapping. If VNet_CIDR A overlaps B and B overlaps C, then {A, B, C} form one conflict group even if A and C do not directly overlap. Exactly one VNet_CIDR per conflict group is **kept**; the rest are **Excluded VNet_CIDRs**.

### Conflict Resolution
Priority order for selecting the kept VNet_CIDR within a conflict group:
1. **Production subscription** — VNet_CIDR whose owning subscription name contains `"prod"` (case-insensitive) wins over non-production.
2. **Most subnets** — count of subnets belonging to **that specific VNet_CIDR** (not all subnets of the VNet). More subnets indicates more active use.
3. **Alphabetical** — by subscription name, ascending.

### Excluded VNet_CIDR
A VNet_CIDR that lost conflict resolution. Only subnets belonging to that specific VNet_CIDR are excluded from gap calculation. Other VNet_CIDRs on the same VNet remain fully active. Excluded subnets are emitted in the CSV as `DUP_EXCL_VNET` rows.

### DUP_EXCL_VNET row
A CSV row for a subnet belonging to an **Excluded VNet_CIDR**. `gap` column = `"DUP_EXCL_VNET"`. `subnet_name` = `"{original_subnet_name} [DUP of VNET {winner_vnet_name}]"`. All other fields are populated normally.

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
A diagram showing all VNets as labelled nodes grouped by **Subscription Island**. Produced in two formats:

- **Mermaid** (`subnets-YYYY-MM-DD-peering.md`): Node labels use `SubscriptionName/VNetName` with the CIDR on a second line. VNets are grouped into `subgraph` blocks by Subscription Island.
- **Graphviz DOT** (`net_YYYY-MM-DD_peering.dot`): VNet node label — header line: `VNET: <B><name></B> VNet_CIDRs: <cidr1>, <cidr2>` (bold name, all VNet_CIDRs comma-separated). Below the header: one line per subnet (`Subnet:<name> CIDR:<cidr>`), ordered by VNet_CIDR start IP then subnet IP within each group. Gateway VNets retain their VNG annotation: `└ VNG:<name> BGP:ASN:<asn>` on its own bold-red line after the GatewaySubnet entry. Each Subscription Island cluster contains nested per-**Subscription** sub-clusters (white fill, dashed border).

**On-Premises (LNG) nodes** — placed at the top level, outside all Island clusters. One node per distinct set of Local Network Gateways. Label format:
```
🌐 LNG:<name>
PubIP:<ip>           (omitted if unknown)
BGP ASN:<asn> Peer:<ip>   (omitted if BGP disabled)
<cidr>
...
```
When a gateway VNet has no LNG data, the fallback label is `🌐 GatewaySubnet: <vnet-name>`.

Gateway VNets have a dotted edge to their LNG node. Standalone VNets (no peerings) appear as single-node subgraphs.

Edge rendering rules:
- **Both sides `Connected`** → single bidirectional arrow (`A <--> B`)
- **Asymmetric or broken** (one side `Disconnected` or `Initiated`) → single directed arrow from the connected side with a stop/cross at the remote end (`A --x B`), styled red via `linkStyle`

---

## Key Invariants

- **Gap finder invariant**: All subnets passed to the gap finder for a given VNet_CIDR must be in non-decreasing IP order with no CIDR overlaps. Violated if subnets from overlapping VNet_CIDRs are mixed.
- **Gap block boundary invariant**: A gap block must not cross a VNet_CIDR Boundary. Every gap row has exactly one label: `-vgap-` (inside a VNet_CIDR) or `-gap-` (outside all VNet_CIDRs).
- **Excluded subnets stay in `Data`**: They are marked with `Subnet.excluded_by = Some(winner_vnet_name)` and skipped by the gap finder, but still emitted in the CSV.
- **No hardcoded exclusion list**: The old `filter_excluded_vnet_cidrs` / `default_vnet_cidrs_to_exclude` mechanism is removed. All conflicts are handled by generic overlap detection.
