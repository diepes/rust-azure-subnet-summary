//! Application pipeline — data processing and output generation.
//!
//! Provides [`run`] which orchestrates the full pipeline from fetched
//! [`AzureData`] to output files. [`SvgRenderer`] is an injectable seam
//! so SVG rendering can be tested in isolation.

use crate::{
    azure::AzureData,
    check_for_duplicate_subnets,
    output::{
        build_topology, subnet_print, validate_dot_file, write_peering_diagram, write_peering_dot,
    },
    processing::{
        de_duplicate_subnets, find_overlapping_vnets, get_vnets, log_overlapping_vnets,
        print_vnets, resolve_overlapping_vnets,
    },
};
use clap::Parser;
use std::collections::HashSet;
use std::error::Error;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;

/// CLI arguments for azure-subnet-summary.
#[derive(Parser, Debug)]
#[command(
    name = "azure-subnet-summary",
    about = "Summarise Azure subnets and IP gaps"
)]
pub struct Args {
    /// Minimum gap-block mask (smaller number = bigger blocks).
    /// /4 means gaps up to a /4 are emitted as a single row.
    #[arg(long, default_value_t = 4, value_name = "N")]
    pub gap_mask: u8,

    /// Comma-separated diagram outputs to generate.
    /// Accepted values: md (Mermaid), dot (Graphviz DOT), svg (DOT + render).
    /// Example: --diagram md,svg   --diagram dot   --diagram svg
    #[arg(long, default_value = "md,svg", value_name = "TYPES")]
    pub diagram: String,
}

/// Injectable SVG rendering seam.
///
/// Receives the path to a validated DOT file and the desired SVG output path.
/// The renderer is responsible for invoking Graphviz (locally or via Docker).
pub trait SvgRenderer {
    fn render(&self, dot_file: &str, svg_file: &str);
}

/// Production renderer: tries local `dot` engines, falls back to Docker.
pub struct GraphvizRenderer;

impl SvgRenderer for GraphvizRenderer {
    fn render(&self, dot_file: &str, svg_file: &str) {
        render_svg(dot_file, svg_file);
    }
}

/// Parse `--diagram` value into a set of lowercase tokens.
fn parse_diagram_types(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Execute the full output pipeline for the fetched Azure data.
///
/// Writes all output files into a `report-<date>` subdirectory (created if it
/// does not exist) and calls `renderer` only when the `svg` diagram type is
/// requested.
pub fn run(data: AzureData, args: &Args, renderer: &dyn SvgRenderer) -> Result<(), Box<dyn Error>> {
    let diagram_types = parse_diagram_types(&args.diagram);

    let cache_source = if data.subnets.from_cache {
        format!("existing cache '{}'", data.subnets.cache_file)
    } else {
        format!("Azure (new cache written to '{}')", data.subnets.cache_file)
    };
    let mut subnets = data.subnets.data;
    subnets.data.sort_by_key(|s| s.subnet_cidr);

    let peering_data = data.peering_edges;
    let local_gw_data = data.local_gateways;
    let vwan_data = data.vwan;

    // Check for and log overlapping VNet CIDRs
    let conflicts = find_overlapping_vnets(&subnets);
    log_overlapping_vnets(&conflicts);

    // Filter overlapping VNets (production subscription wins)
    let cr_out = resolve_overlapping_vnets(subnets);
    for e in &cr_out.excluded {
        log::warn!(
            "Excluding VNet '{}' — overlaps with kept VNet '{}'",
            e.subnet.vnet_name,
            e.winner_vnet_name,
        );
    }
    let subnets = cr_out.active;

    let subnets = de_duplicate_subnets(subnets, None)?;
    check_for_duplicate_subnets(&subnets)?;

    // Create the dated report subdirectory
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let report_dir = format!("report-{date_str}");
    std::fs::create_dir_all(&report_dir)?;
    let report_path = Path::new(&report_dir);
    log::info!("Writing output to '{report_dir}/'");

    // Output subnet CSV + duplicates.md (both written by subnet_print)
    let csv_file = subnet_print(
        &subnets,
        &cr_out.excluded,
        args.gap_mask,
        &vwan_data.data,
        report_path,
    )?;

    // Build peering topology once; pass to both diagram writers.
    let topo = build_topology(
        &peering_data.data,
        &subnets,
        &local_gw_data.data,
        &vwan_data.data,
    );

    if diagram_types.contains("md") {
        let peering_file = report_path
            .join(format!("net_{date_str}_peering.md"))
            .to_string_lossy()
            .into_owned();
        write_peering_diagram(&topo, &peering_file)?;
        log::info!("Peering diagram written to '{peering_file}'");
    }

    // Generate DOT file (needed for both dot and svg outputs).
    let peering_dot_file = if diagram_types.contains("dot") || diagram_types.contains("svg") {
        let f = report_path
            .join(format!("net_{date_str}_peering.dot"))
            .to_string_lossy()
            .into_owned();
        write_peering_dot(&topo, &f)?;
        log::info!("Peering DOT diagram written to '{f}'");
        Some(f)
    } else {
        None
    };

    // Output VNet summary
    let vnets = get_vnets(&subnets)?;
    print_vnets(&vnets, &cr_out.excluded)?;

    log::info!("Complete: Generated '{}' from {}", csv_file, cache_source);

    // SVG rendering last so errors appear at the bottom of terminal output.
    if let Some(ref dot_file) = peering_dot_file {
        if diagram_types.contains("svg") {
            let peering_svg_file = report_path
                .join(format!("net_{date_str}_peering.svg"))
                .to_string_lossy()
                .into_owned();
            renderer.render(dot_file, &peering_svg_file);
        }
    }

    Ok(())
}

// ── SVG rendering internals ──────────────────────────────────────────────────

const DOCKER_IMAGE: &str = "minidocks/graphviz";

/// Layout engines tried in order. `fdp` gives the best visual layout;
/// `sfdp` is the scalable fallback for very large/complex graphs.
const DOT_ENGINES: &[&str] = &["fdp", "sfdp", "neato"];

/// Describes why a child process exited (exit code or signal on Unix).
fn exit_description(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit code {code}");
    }
    #[cfg(unix)]
    if let Some(sig) = status.signal() {
        let name = match sig {
            1 => "SIGHUP",
            2 => "SIGINT",
            3 => "SIGQUIT",
            6 => "SIGABRT",
            9 => "SIGKILL",
            11 => "SIGSEGV (segfault / crash)",
            13 => "SIGPIPE",
            15 => "SIGTERM",
            _ => "unknown signal",
        };
        return format!("killed by signal {sig} ({name})");
    }
    "unknown failure".to_string()
}

/// Render a DOT file to SVG via local `dot` or Docker fallback.
fn render_svg(dot_file: &str, svg_file: &str) {
    if let Err(msg) = validate_dot_file(dot_file) {
        eprintln!("ERROR: DOT validation failed — {msg}");
        log::error!("DOT validation failed — {msg}");
        return;
    }

    // Try local `dot` with each engine in order
    let mut dot_installed = false;
    for engine in DOT_ENGINES {
        let local = std::process::Command::new("dot")
            .args([&format!("-K{engine}"), "-Tsvg", dot_file, "-o", svg_file])
            .output();
        match local {
            Ok(out) if out.status.success() => {
                log::info!("SVG written to '{svg_file}' (via local dot -{engine})");
                return;
            }
            Ok(out) => {
                dot_installed = true;
                let stderr = String::from_utf8_lossy(&out.stderr);
                let why = exit_description(&out.status);
                let msg = format!("dot -{engine} {why}: {stderr}");
                log::warn!("{msg}");
                eprintln!("WARN: {msg}");
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => {
                log::warn!("Could not launch local dot: {e} — trying Docker");
                break;
            }
        }
    }

    if dot_installed {
        let engines = DOT_ENGINES.join(", ");
        eprintln!("ERROR: local dot could not render '{dot_file}' (tried engines: {engines})");
        return;
    }

    // Fall back to Docker
    let cwd = match std::env::current_dir() {
        Ok(p) => p.display().to_string(),
        Err(e) => {
            let msg = format!("Could not determine current directory for Docker volume: {e}");
            log::warn!("{msg}");
            eprintln!("ERROR: {msg}");
            return;
        }
    };
    log::info!("Local dot not available — trying Docker ({DOCKER_IMAGE})");

    for engine in DOT_ENGINES {
        let manual_cmd = format!(
            "docker run --rm -v \"{cwd}:/data\" -w /data {DOCKER_IMAGE} dot -K{engine} -Tsvg {dot_file} -o {svg_file}"
        );

        let result = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "-v",
                &format!("{cwd}:/data"),
                "-w",
                "/data",
                DOCKER_IMAGE,
                "dot",
                &format!("-K{engine}"),
                "-Tsvg",
                dot_file,
                "-o",
                svg_file,
            ])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                log::info!("SVG written to '{svg_file}' (via Docker -{engine})");
                return;
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let why = exit_description(&out.status);
                let msg =
                    format!("Docker dot -{engine} {why}: {stderr}\nRun manually:\n  {manual_cmd}");
                log::warn!("{msg}");
                eprintln!("WARN: {msg}");
            }
            Err(e) => {
                let msg = format!(
                    "Could not launch docker ({e}). Install graphviz or Docker, then run:\n  {manual_cmd}"
                );
                log::warn!("{msg}");
                eprintln!("WARN: {msg}");
                return;
            }
        }
    }
    let engines = DOT_ENGINES.join(", ");
    eprintln!("ERROR: Docker dot could not render '{dot_file}' (tried engines: {engines})");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::{fetch_azure_data, FetchConfig};
    use std::cell::RefCell;

    struct SpyRenderer {
        was_called: RefCell<bool>,
    }

    impl SpyRenderer {
        fn new() -> Self {
            Self {
                was_called: RefCell::new(false),
            }
        }
        fn called(&self) -> bool {
            *self.was_called.borrow()
        }
    }

    impl SvgRenderer for SpyRenderer {
        fn render(&self, _dot_file: &str, _svg_file: &str) {
            *self.was_called.borrow_mut() = true;
        }
    }

    fn test_azure_data() -> AzureData {
        let config = FetchConfig {
            subnet_cache: Some("src/tests/test_data/subnet_test_cache_01.json".to_string()),
            peering_cache: Some("src/tests/test_data/peering_test_cache_01.json".to_string()),
            local_gateway_cache: Some(
                "src/tests/test_data/local_gateway_test_cache_01.json".to_string(),
            ),
            vwan_cache: Some("src/tests/test_data/vwan_test_cache_01.json".to_string()),
            ..FetchConfig::default()
        };
        fetch_azure_data(&config).expect("test fixture fetch failed")
    }

    #[test]
    fn renderer_called_when_svg_in_diagram_types() {
        let args = Args {
            gap_mask: 4,
            diagram: "svg".to_string(),
        };
        let renderer = SpyRenderer::new();
        run(test_azure_data(), &args, &renderer).expect("pipeline run failed");
        assert!(
            renderer.called(),
            "renderer should have been called for svg"
        );
    }

    #[test]
    fn renderer_not_called_when_svg_not_in_diagram_types() {
        let args = Args {
            gap_mask: 4,
            diagram: "md".to_string(),
        };
        let renderer = SpyRenderer::new();
        run(test_azure_data(), &args, &renderer).expect("pipeline run failed");
        assert!(
            !renderer.called(),
            "renderer should NOT have been called without svg"
        );
    }
}
