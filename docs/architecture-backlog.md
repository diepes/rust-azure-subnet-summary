# Architecture Uplift Backlog

These candidates were surfaced during an `improve-codebase-architecture` session.
Recorded here so work can resume after fixing the current issue.

---

## Candidate 1 — Lift `PeeringTopology` to `pub(crate)`, build once ✅ DONE

**Files:** `src/output/peering_topology.rs`, `src/output/peering_dot.rs`, `src/output/peering_diagram.rs`, `src/main.rs`

**Problem:** Both diagram writers (`write_peering_dot`, `write_peering_diagram`) accept `(edges, subnets, local_gateways)` — three raw Azure data collections — and each calls `build_topology` independently. The caller must know to assemble and pass the same three inputs to both writers. Because `PeeringTopology` is `pub(super)`, no test can construct a topology directly and inject it; tests must fabricate full `PeeringEdge` and `Data` objects to exercise rendering logic.

**Solution:** Make `PeeringTopology` and `build_topology` `pub(crate)`. Diagram writers take `&PeeringTopology` as their only domain input. `main.rs` builds the topology once and passes it to both. Three-input coordination disappears from the call site. Tests can construct a `PeeringTopology` with hand-crafted islands and assert on DOT/Mermaid output directly.

---

## Candidate 2 — Remove `Subnet.excluded_by` and pipeline state from the domain model ✅ DONE

**Files:** `src/models/subnet.rs`, `src/processing/overlap.rs`, `src/output/csv.rs`, `src/output/dup_report.rs`, `src/processing/vnet.rs`

**Problem:** `Subnet.excluded_by: Option<String>` is a Conflict Resolution decision stored on the raw Azure data model. Every output module must know that `excluded_by.is_some()` means "this Subnet lost Conflict Resolution." The fields `gap`, `src_index`, and `block_id` are similarly pipeline-internal values on the same struct. The Excluded VNet concept is expressed only as a runtime `Option` check, not as a type.

**Solution:** Replace with a `ConflictResolutionOutput` type: `active: Vec<Subnet>` and `excluded: Vec<ExcludedSubnet>` (carrying winner name alongside the subnet). Remove `excluded_by`, `gap`, `src_index`, `block_id` from `Subnet`. Output modules receive typed input; the "production wins" rule becomes private to the Conflict Resolution module.

---

## Candidate 3 — `GapFinder` as a push-based accumulator ✅ DONE

**Files:** `src/processing/gap_finder.rs`, `src/output/csv.rs`

**Problem:** `process_subnet_row` returns `(Ipv4Addr, PrevVnetContext, Vec<SubnetPrintRow>)` — the caller threads `next_ip` and `PrevVnetContext` forward between every call. `output/csv.rs` manually manages this state in a loop. The sorted-input invariant is enforced only by convention; wrong-order inputs silently produce wrong gap rows.

**Solution:** A `GapFinder` struct with a `push(&mut self, subnet: &Subnet) -> Vec<SubnetPrintRow>` method. It owns `next_ip` and `PrevVnetContext` internally. Callers push subnets and collect rows with no external state to thread. The sorted-input invariant is asserted inside `push`.

---

## Candidate 4 — Delete the legacy module graveyard ✅ DONE

**Files:** `src/ipv4.rs`, `src/subnet_struct.rs`, `src/graph_read_subnet_data.rs`, `src/de_duplicate_subnets.rs`, `src/subnet_add_row.rs`, `src/subnet_print.rs`

**Problem:** Six files are parallel implementations of modern equivalents in `models/`, `processing/`, and `azure/`. They duplicate `Ipv4`, `Subnet`, `process_subnet_row`, cache reading, and dedup. `lib.rs` re-exports parts of the legacy path, keeping it alive. New readers cannot tell which `Ipv4` or `process_subnet_row` to follow.

**Solution:** Apply the deletion test — delete all six files, trace remaining callers in `lib.rs` and `tests/`, and switch them to modern equivalents. Complexity vanishes rather than spreading.

---

## Candidate 5 — Single `fetch_azure_data` function replacing three cache calls ✅ DONE

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

---

# Session 2 — Surfaced 2026-06-01

---

## Candidate 6 — Generic cache module

**Files:** `src/azure/cache.rs`, `src/azure/local_gateway_cache.rs`, `src/azure/peering_cache.rs`, `src/azure/vwan_cache.rs`

**Problem:** Four shallow modules repeat the same pattern verbatim: check a dated file, fall back to an Azure query, deserialise JSON, return a `CacheResult`. Only the data type, filename stem, and query function differ. The deletion test confirms shallowness — delete any one and its logic moves verbatim to the caller. Cache expiry or error-handling fixes must be applied four times.

**Solution:** A single generic `AzureCache<T>` module parameterised over the fetched data type. Each data source provides only what varies: query function, cache filename stem, and deserialise target. The four existing modules become thin adapters (or vanish entirely).

**Benefits:** Cache expiry and error-handling logic fixed once (locality). New Azure data sources get caching for free (leverage). Cache behaviour becomes testable through one interface.

**Design decisions (grilling session 2026-06-01):**
- Date-stamping (`net_YYYY-MM-DD_cache_<key>.json`) is owned by the generic module — all sources share the same convention.
- Each source implements an `AzureSource` trait with an associated `Data` type, a `cache_key() -> &str`, and an async `fetch() -> Result<Self::Data>`.
- A single shared `CacheStatus` enum (`Hit`, `Miss`, `Stale`) replaces the four separate status types.
- Error policy: always propagate — callers are responsible for logging.

```rust
trait AzureSource {
    type Data: Serialize + DeserializeOwned;
    fn cache_key(&self) -> &str;
    async fn fetch(&self) -> Result<Self::Data>;
}

async fn load<S: AzureSource>(source: &S) -> Result<(S::Data, CacheStatus)>

enum CacheStatus { Hit, Miss, Stale }
```

---

## Candidate 7 — Paginated Azure query seam

**Files:** `src/azure/graph.rs`, `src/azure/peering_graph.rs`, `src/azure/local_gateway.rs`, `src/azure/vwan_graph.rs`

**Problem:** All four Azure query modules repeat the same pagination loop: send query, inspect `skip_token`, accumulate pages, rate-limit. The pagination protocol leaks into every module's implementation, and each carries its own retry/sleep policy. No module can be tested without executing the full loop.

**Design decisions (grilling session 2026-06-01):**
- Separate seam from Candidate 6 — operates at the transport layer, one level below the cache layer.
- Layering: `load<S: AzureSource>` → `source.fetch()` → `paginate(query, sleep)` → az CLI / SDK.
- `paginate` returns `Vec<serde_json::Value>` of **flattened rows** (all pages merged). Each query module only parses typed structs — it never sees `skip_token` or page boundaries.
- Sleep duration is passed in by the caller (`paginate(query: &str, sleep: Duration) -> Result<Vec<serde_json::Value>>`). Tests pass `Duration::ZERO`; production passes `SLEEP_MSEC`. `config::SLEEP_MSEC` stays but is the single place callers read from.

---

## Candidate 8 — `main.rs` pipeline depth

**Files:** `src/main.rs`

**Problem:** `main.rs` is a shallow coordinator: it knows about output formats, Graphviz invocation, Docker fallback, SVG rendering, and data-processing order all in one function body. There is no seam to test against; the deletion test shows all that complexity just moves to any future orchestrator.

**Solution:** Extract a `pipeline` module (in the lib crate) with a free function `run(data: AzureData, args: &Args) -> Result<()>`. SVG rendering becomes a `SvgRenderer` trait with two adapters (`LocalGraphviz`, `DockerGraphviz`); `main.rs` wires up the real adapters and passes them in. `main.rs` becomes: parse args → fetch → call `pipeline::run`.

**Benefits:** The pipeline is exercisable with mock `AzureData` (leverage). SVG fallback logic has locality — one place to fix or extend. `main.rs` stops being a test-blind wall.

**Design decisions (grilling session 2026-06-01):**
- `pipeline::run` lives in the lib crate (`src/pipeline.rs`) so tests can call it directly without touching `main.rs`.
- Config is the clap `Args` struct — no separate translation layer (lib crate already depends on clap).
- SVG rendering is a `SvgRenderer` trait (`render(dot: &str) -> Result<Vec<u8>>`). Two real adapters: `LocalGraphviz`, `DockerGraphviz`. Tests inject a stub. `main.rs` owns the fallback wiring.

```rust
// src/pipeline.rs
pub async fn run(data: AzureData, args: &Args, renderer: &dyn SvgRenderer) -> Result<()>

trait SvgRenderer {
    fn render(&self, dot: &str) -> Result<Vec<u8>>;
}
```

---

## Candidate 9 — `output::csv` row-stream decomposition

**Files:** `src/output/csv.rs`, `src/processing/gap_finder.rs`

**Problem:** The CSV writer mixes six concerns in one function: sort ordering, Gap injection, Excluded VNet_CIDR row injection, vWAN hub row injection, CSV field formatting, and side-effect report generation. Callers must understand all six to call it correctly. Any change to row ordering or a new row type (e.g. a new Azure annotation) requires editing this single overloaded function.

**Solution:** Separate a row-stream producer (a sorted, gap-and-dup-merged iterator of `SubnetPrintRow`) from the CSV formatter (writes whatever rows it receives). The row-stream becomes the deep module with a small interface; the CSV formatter becomes a thin adapter.

**Benefits:** Gap and duplicate injection logic is testable without writing a file (leverage). Adding a new row type only touches the stream producer (locality). The formatter is trivially swappable (e.g. to emit JSON instead of CSV).

**Design decisions (grilling session 2026-06-01):**
- Row producer is a free function `build_rows(subnets, excluded, vwan) -> Vec<SubnetPrintRow>` living in `output::` (CSV shape is output-layer knowledge).
- Returns `Vec` (not iterator) — simpler to implement and easier to assert on in tests; the allocation cost is trivial at subnet scale.
- The duplicates report remains a side effect inside the CSV writer (`subnet_print`) — it is an output concern, not a row-building concern.

```rust
// src/output/csv.rs
pub fn build_rows(
    subnets: &[Subnet],
    excluded: &[ExcludedSubnet],
    vwan: &VWanData,
) -> Vec<SubnetPrintRow>

pub fn subnet_print(rows: &[SubnetPrintRow], path: &Path) -> Result<()>
```

---

## Candidate 10 — `GapIterator`: decouple Gap-Finding Loop state machine from row shaping

**Files:** `src/processing/gap_finder.rs`

**Problem:** `GapFinder` intertwines two concerns: the Gap-Finding Loop state machine (the core IP-arithmetic invariant documented in CONTEXT.md) and Azure-specific row formatting (field population, host-count math). Bugs in the gap-finding invariant and bugs in row construction are impossible to isolate in tests — both live in the same type.

**Solution:** A pure `gaps()` function that accepts a sorted slice of VNet_CIDRs and Subnets and yields typed `GapEvent` values. Row construction lives downstream in `build_rows` (Candidate 9). The Gap-Finding Loop invariant maps directly to one clean test surface.

**Benefits:** The gap-finding invariant from CONTEXT.md can be verified with minimal fixtures — just IP ranges, no Azure fields (leverage). Changes to row format cannot break gap logic (locality). The CONTEXT.md invariant and the code are in 1:1 correspondence.

**Design decisions (grilling session 2026-06-01):**
- Inputs: `vnet_cidrs: &[VnetCidr]` — a single sorted structure. `VnetCidr` carries `cidr: Ipv4`, VNet metadata (`vnet_name`, `subscription_id`, etc.), and `subnets: Vec<Subnet>` (moved in from the flat subnet list, sorted by subnet start IP). The flat `Vec<Subnet>` is consumed once when building `Vec<VnetCidr>`; from that point on there is one data structure.
- Returns `Vec<GapEvent<'_>>` — eager, consistent with Candidate 9.
- `GapKind::Vnet(&'a VnetCidr)` carries the owning VNet_CIDR struct — all metadata for the gap row is available directly, no inference needed.
- `GapKind::Gap` carries no extra data — inter-VNet_CIDR gaps have no owning VNet context.
- Subnet events carry `&'a Subnet` so the row builder has all fields without a lookup.
- "A Subnet belongs to exactly one VNet_CIDR" (CONTEXT.md invariant) is now structural — enforced by the type, not just convention.

```rust
// src/processing/gap_finder.rs
pub fn gaps(vnet_cidrs: &[VnetCidr]) -> Vec<GapEvent<'_>>

pub struct GapEvent<'a> {
    pub cidr: Ipv4,
    pub kind: GapKind<'a>,
}

pub enum GapKind<'a> {
    Gap,                   // between VNet_CIDRs (no owning VNet)
    Vnet(&'a VnetCidr),    // inside a VNet_CIDR
    Subnet(&'a Subnet),    // existing subnet
}

pub struct VnetCidr {
    pub cidr: Ipv4,
    pub vnet_name: String,
    pub subscription_id: String,
    // other fields required for CSV row generation
    pub subnets: Vec<Subnet>,  // moved in, sorted by subnet start IP
}
```

---

## Priority order — Session 2 (recommended)
1. Candidate 6 — generic cache module (pure boilerplate reduction, low risk)
2. Candidate 7 — paginated query seam (removes copy-paste protocol, improves testability)
3. Candidate 9 — CSV row-stream decomposition (high test leverage, isolated change)
4. Candidate 10 — `GapIterator` state machine isolation (invariant clarity, pairs well with 9)
5. Candidate 8 — pipeline depth in `main.rs` (largest structural change, highest payoff last)
