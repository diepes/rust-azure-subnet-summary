# Subnet.vnet_cidr is a single Ipv4, not Vec<Ipv4>

Azure's API returns `vnet_cidr` as a JSON array, but each subnet row in our cache always contains exactly one entry — the specific VNet_CIDR that contains this subnet. We changed the field type from `Vec<Ipv4>` to `Ipv4` to encode the domain invariant ("a Subnet belongs to exactly one VNet_CIDR") in the type system. A custom serde deserializer unwraps the single-element JSON array on load; the cache file format is unchanged.

## Considered Options

- **Keep `Vec<Ipv4>`** — no deserializer change, but the invariant is invisible to the compiler and the original bug (treating the vec as "all CIDRs of the parent VNet") could silently recur.
- **Plain `Ipv4`** — chosen. Callers that previously iterated the vec now use the value directly; any future attempt to treat it as a multi-element collection fails at compile time.
