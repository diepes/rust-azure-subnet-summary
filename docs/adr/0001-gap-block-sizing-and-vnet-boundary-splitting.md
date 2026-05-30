# Gap block sizing: remove /16 floor, split at VNet CIDR boundaries

The original gap-finding code capped gap blocks at `/16`, producing hundreds of synthetic `-gap-` rows for large unallocated ranges (e.g. a single `/9` gap would generate ~128 rows). We removed the artificial floor by defaulting `--gap-mask` to `4`, letting IP alignment determine the maximum block size (effectively `/8` within `10.x.x.x` space).

We also decided that gap blocks must not cross VNet CIDR boundaries. Without splitting, a single large gap row could start inside a VNet (correct label: `-vgap-`) but extend past the VNet's broadcast into unowned space (correct label: `-gap-`), making the label misleading for capacity planning. Splitting at boundaries ensures every gap row carries an unambiguous label.

## Considered Options

- **One big gap row, no splitting** — simpler implementation, but the `-vgap-` / `-gap-` label would apply to only the start IP, not the full extent of the row.
- **Configurable but default `/16`** — preserves backward-compatible output; rejected because the default was the problem.

## Consequences

- CSV row count for large unallocated ranges drops significantly (e.g. a gap spanning 10.0.0.0–10.127.255.255 becomes ~1–3 rows instead of ~128).
- Gap blocks stop exactly at VNet CIDR boundaries, so `-vgap-` / `-gap-` labels are always accurate.
- `--gap-mask <N>` lets operators restore the old dense output (`--gap-mask 16`) or go even finer.
