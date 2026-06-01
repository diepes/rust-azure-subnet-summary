//! Azure Subnet Summary - Main entry point
//!
//! This tool queries Azure Resource Graph to get subnet information,
//! identifies gaps in IP address allocation, and outputs a CSV summary.

use azure_subnet_summary::{
    azure::{
        read_local_gateway_cache_with_status, read_peering_cache_with_status,
        read_vwan_cache_with_status, LocalGatewayCacheResult, PeeringCacheResult, VWanCacheResult,
    },
    check_for_duplicate_subnets, get_sorted_subnets_with_status,
    output::{
        subnet_print, validate_dot_file, write_duplicates_md, write_peering_diagram,
        write_peering_dot,
    },
    processing::{
        de_duplicate_subnets, filter_overlapping_vnets, find_overlapping_vnets, get_vnets,
        log_overlapping_vnets, print_vnets,
    },
};
use clap::Parser;
use std::collections::HashSet;
use std::error::Error;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;

/// Azure Subnet Summary - maps IP allocation and identifies free gaps.
#[derive(Parser, Debug)]
#[command(
    name = "azure-subnet-summary",
    about = "Summarise Azure subnets and IP gaps"
)]
struct Args {
    /// Minimum gap-block mask (smaller number = bigger blocks).
    /// /4 means gaps up to a /4 (covering 1/16th of IPv4 space) are emitted
    /// as a single row instead of many /16 rows.
    #[arg(long, default_value_t = 4, value_name = "N")]
    gap_mask: u8,

    /// Comma-separated diagram outputs to generate.
    /// Accepted values: md (Mermaid), dot (Graphviz DOT), svg (DOT + Docker render).
    /// Example: --diagram md,svg   --diagram dot   --diagram svg
    #[arg(long, default_value = "md,svg", value_name = "TYPES")]
    diagram: String,
}

/// Parse `--diagram` value into a set of lowercase tokens.
fn parse_diagram_types(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

const DOCKER_IMAGE: &str = "minidocks/graphviz";

/// Run graphviz to render a DOT file to SVG.
///
/// Describes why a child process exited — exit code or signal (on Unix).
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

/// Layout engines to try in order.  `fdp` gives the best visual layout —
/// hub nodes naturally sit in the middle of the VNets they connect — so it
/// is tried first.  `sfdp` (the scalable variant) is the fallback for cases
/// where `fdp` crashes on very large or complex graphs.
const DOT_ENGINES: &[&str] = &["fdp", "sfdp", "neato"];

fn render_svg_via_docker(dot_file: &str, svg_file: &str) {
    // ── Pre-validate the DOT file before invoking Graphviz ─────────────────
    if let Err(msg) = validate_dot_file(dot_file) {
        eprintln!("ERROR: DOT validation failed — {msg}");
        log::error!("DOT validation failed — {msg}");
        return;
    }

    // ── Try local `dot` with each engine in order ──────────────────────────
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
                // try next engine
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                // dot not installed — skip to Docker
                break;
            }
            Err(e) => {
                log::warn!("Could not launch local dot: {e} — trying Docker");
                break;
            }
        }
    }

    if dot_installed {
        // dot was found but all engines failed; Docker won't help
        let engines = DOT_ENGINES.join(", ");
        eprintln!("ERROR: local dot could not render '{dot_file}' (tried engines: {engines})");
        return;
    }

    // ── Fall back to Docker — try each engine in order ────────────────────
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
                // try next engine
            }
            Err(e) => {
                let msg = format!(
                    "Could not launch docker ({e}). Install graphviz or Docker, then run:\n  {manual_cmd}"
                );
                log::warn!("{msg}");
                eprintln!("WARN: {msg}");
                return; // docker itself not found — no point retrying
            }
        }
    }
    let engines = DOT_ENGINES.join(", ");
    eprintln!("ERROR: Docker dot could not render '{dot_file}' (tried engines: {engines})");
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let diagram_types = parse_diagram_types(&args.diagram);

    // Initialize logging - fall back to default console logger if config file is missing
    if log4rs::init_file("log4rs.yml", Default::default()).is_err() {
        let stdout = log4rs::append::console::ConsoleAppender::builder().build();
        let config = log4rs::Config::builder()
            .appender(log4rs::config::Appender::builder().build("stdout", Box::new(stdout)))
            .build(
                log4rs::config::Root::builder()
                    .appender("stdout")
                    .build(log::LevelFilter::Info),
            )?;
        log4rs::init_config(config)?;
    }
    dotenv::dotenv().ok();

    log::info!("#Start main()");

    // Fetch and process subnet data (with cache status)
    let cache_result = get_sorted_subnets_with_status(None)?;
    let cache_source = if cache_result.from_cache {
        format!("existing cache '{}'", cache_result.cache_file)
    } else {
        format!("Azure (new cache written to '{}')", cache_result.cache_file)
    };
    let data = cache_result.data;

    // Check for and log overlapping VNet CIDRs
    let conflicts = find_overlapping_vnets(&data);
    log_overlapping_vnets(&conflicts);

    // Filter overlapping VNets (production subscription wins; marks losers with excluded_by)
    // This must happen before gap-finding, which assumes subnets are non-overlapping
    let data = filter_overlapping_vnets(data, true)?;

    let data = de_duplicate_subnets(data, None)?;
    check_for_duplicate_subnets(&data)?;

    // Load vWAN data early so hub CIDRs can be included in the subnet CSV.
    let VWanCacheResult {
        data: vwan_data,
        from_cache: vwan_from_cache,
        cache_file: vwan_cache_file,
    } = read_vwan_cache_with_status(None)?;
    if vwan_from_cache {
        log::info!("vWAN data read from cache '{vwan_cache_file}'");
    } else {
        log::info!("vWAN data fetched from Azure (cache '{vwan_cache_file}')");
    }

    // Output subnet summary (includes vWAN hub CIDRs as reserved IP space)
    let csv_file = subnet_print(&data, args.gap_mask, &vwan_data.data)?;

    // Output duplicate VNet report
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let dup_file = format!("net_{date_str}_duplicates.md");
    write_duplicates_md(&data, &dup_file)?;
    log::info!("Duplicates report written to '{dup_file}'");

    // Output peering diagram
    let PeeringCacheResult {
        data: peering_data,
        from_cache: peering_from_cache,
        cache_file: peering_cache_file,
    } = read_peering_cache_with_status(None)?;
    let peering_source = if peering_from_cache {
        format!("existing cache '{peering_cache_file}'")
    } else {
        format!("Azure (new cache written to '{peering_cache_file}')")
    };

    let LocalGatewayCacheResult {
        data: local_gw_data,
        from_cache: lgw_from_cache,
        cache_file: lgw_cache_file,
    } = read_local_gateway_cache_with_status(None)?;
    if lgw_from_cache {
        log::info!("Local gateway data read from cache '{lgw_cache_file}'");
    } else {
        log::info!("Local gateway data fetched from Azure (cache '{lgw_cache_file}')");
    }

    if diagram_types.contains("md") {
        let peering_file = format!("net_{date_str}_peering.md");
        write_peering_diagram(
            &peering_data.data,
            &data,
            &local_gw_data.data,
            &vwan_data.data,
            &peering_file,
        )?;
        log::info!("Peering diagram written to '{peering_file}' from {peering_source}");
    }

    // Generate DOT file (needed for both dot and svg outputs).
    // Store the file name so we can render SVG after all other output.
    let peering_dot_file = if diagram_types.contains("dot") || diagram_types.contains("svg") {
        let f = format!("net_{date_str}_peering.dot");
        write_peering_dot(
            &peering_data.data,
            &data,
            &local_gw_data.data,
            &vwan_data.data,
            &f,
        )?;
        log::info!("Peering DOT diagram written to '{f}'");
        Some(f)
    } else {
        None
    };

    // Output VNet summary
    let vnets = get_vnets(&data)?;
    print_vnets(&vnets, None)?;

    // Final summary
    log::info!("Complete: Generated '{}' from {}", csv_file, cache_source);

    // SVG rendering runs last so any dot error appears at the bottom of the
    // terminal output rather than being buried by the VNet list above.
    if let Some(ref dot_file) = peering_dot_file {
        if diagram_types.contains("svg") {
            let peering_svg_file = format!("net_{date_str}_peering.svg");
            render_svg_via_docker(dot_file, &peering_svg_file);
        }
    }

    Ok(())
}
