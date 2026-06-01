# Architecture Uplift Backlog

These candidates were surfaced during an `improve-codebase-architecture` session.
Recorded here so work can resume after fixing the current issue.

---

## Candidate 1 — Lift `PeeringTopology` to `pub(crate)`, build once

**Files:** `src/output/peering_topology.rs`, `src/output/peering_dot.rs`, `src/output/peering_diagram.rs`, `src/main.rs`

**Problem:** Both diagram writers (`write_peering_dot`, `write_peering_diagram`) accept `(edges, subnets, local_gateways)` — three raw Azure data collections — and each calls `build_topology` independently. The caller must know to assemble and pass the same three inputs to both writers. Because `PeeringTopology` is `pub(super)`, no test can construct a topology directly and inject it; tests must fabricate full `PeeringEdge` and `Data` objects to exercise rendering logic.

**Solution:** Make `PeeringTopology` and `build_topology` `pub(crate)`. Diagram writers take `&PeeringTopology` as their only domain input. `main.rs` builds the topology once and passes it to both. Three-input coordination disappears from the call site. Tests can construct a `PeeringTopology` with hand-crafted islands and assert on DOT/Mermaid output directly.

---

## Candidate 2 — Remove `Subnet.excluded_by` and pipeline state from the domain model ✅ DONE

**Files:** `src/models/subnet.rs`, `src/processing/overlap.rs`, `src/output/csv.rs`, `src/output/dup_report.rs`, `src/processing/vnet.rs`

**Problem:** `Subnet.excluded_by: Option<String>` is a Conflict Resolution decision stored on the raw Azure data model. Every output module must know that `excluded_by.is_some()` means "this Subnet lost Conflict Resolution." The fields `gap`, `src_index`, and `block_id` are similarly pipeline-internal values on the same struct. The Excluded VNet concept is expressed only as a runtime `Option` check, not as a type.

**Solution:** Replace with a `ConflictResolutionOutput` type: `active: Vec<Subnet>` and `excluded: Vec<ExcludedSubnet>` (carrying winner name alongside the subnet). Remove `excluded_by`, `gap`, `src_index`, `block_id` from `Subnet`. Output modules receive typed input; the "production wins" rule becomes private to the Conflict Resolution module.

---

## Candidate 3 — `GapFinder` as a push-based accumulator

**Files:** `src/processing/gap_finder.rs`, `src/output/csv.rs`

**Problem:** `process_subnet_row` returns `(Ipv4Addr, PrevVnetContext, Vec<SubnetPrintRow>)` — the caller threads `next_ip` and `PrevVnetContext` forward between every call. `output/csv.rs` manually manages this state in a loop. The sorted-input invariant is enforced only by convention; wrong-order inputs silently produce wrong gap rows.

**Solution:** A `GapFinder` struct with a `push(&mut self, subnet: &Subnet) -> Vec<SubnetPrintRow>` method. It owns `next_ip` and `PrevVnetContext` internally. Callers push subnets and collect rows with no external state to thread. The sorted-input invariant is asserted inside `push`.

---

## Candidate 4 — Delete the legacy module graveyard

**Files:** `src/ipv4.rs`, `src/subnet_struct.rs`, `src/graph_read_subnet_data.rs`, `src/de_duplicate_subnets.rs`, `src/subnet_add_row.rs`, `src/subnet_print.rs`

**Problem:** Six files are parallel implementations of modern equivalents in `models/`, `processing/`, and `azure/`. They duplicate `Ipv4`, `Subnet`, `process_subnet_row`, cache reading, and dedup. `lib.rs` re-exports parts of the legacy path, keeping it alive. New readers cannot tell which `Ipv4` or `process_subnet_row` to follow.

**Solution:** Apply the deletion test — delete all six files, trace remaining callers in `lib.rs` and `tests/`, and switch them to modern equivalents. Complexity vanishes rather than spreading.

---

## Candidate 5 — Single `fetch_azure_data` function replacing three cache calls

**Files:** `src/azure/cache.rs`, `src/azure/peering_cache.rs`, `src/azure/local_gateway_cache.rs`, `src/main.rs`

**Problem:** `main.rs` calls three separate `read_*_cache_with_status` functions, handles three `CacheResult` types, and prints three status blocks. Adding a new Azure dataset forces `main.rs` to grow a fourth block. Coordination cost sits in the orchestrator, not the data-fetching module.

**Solution:** A single `fetch_azure_data(config) -> AzureData` function returning a composite `AzureData { subnets, peering_edges, local_gateways }`. Cache status logging moves inside the function. `main.rs` makes one call. New Azure datasets only change `fetch_azure_data` and `AzureData`, not every orchestrator.

---

## Priority order (recommended)
1. Candidate 4 — legacy deletion (highest unambiguous wins, zero risk)
2. Candidate 1 — `PeeringTopology` seam (highest test leverage)
3. Candidate 3 — `GapFinder` accumulator (clean isolation)
4. Candidate 5 — Azure data fetch composite (reduces orchestration)
5. Candidate 2 — domain model cleanup (largest change, most careful)
