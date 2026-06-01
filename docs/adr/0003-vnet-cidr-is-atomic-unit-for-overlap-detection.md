# VNet_CIDR is the atomic unit for overlap detection and gap-finding

A VNet can have multiple independent address spaces (VNet_CIDRs). Previous code treated the whole VNet as the atomic unit, so when *one* VNet_CIDR conflicted, *all* subnets of that VNet were excluded and only the first-seen CIDR appeared in the peering diagram. We changed the unit of conflict detection, resolution, and gap-finding to the individual VNet_CIDR. Only subnets belonging to a conflicting VNet_CIDR are excluded; the VNet's other address spaces remain fully active.

## Considered Options

- **VNet-level detection** — simpler; the current (buggy) code. Rejected because it over-excludes subnets that live in uncontested address spaces, and mis-labels nodes in the peering diagram.
- **VNet_CIDR-level detection** — chosen. The composite key `(vnet_name, subscription_id, vnet_cidr)` identifies each address space independently. Conflict resolution tie-breaking counts subnets within that specific VNet_CIDR, not all subnets of the VNet.

## Consequences

- The gap-finding loop now iterates VNet_CIDRs (not VNets), processing all subnets within each VNet_CIDR before moving to the next, and filling trailing `-vgap-` rows to the VNet_CIDR's broadcast address.
- The peering diagram node label shows all VNet_CIDRs comma-separated on the header line.
