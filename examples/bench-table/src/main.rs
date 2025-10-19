use comfy_table::{Cell, Row, Table};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use walkdir::WalkDir;

#[derive(Deserialize, Debug)]
struct Estimate {
    point_estimate: f64,
}

#[derive(Deserialize, Debug)]
struct Benchmark {
    mean: Estimate,
}

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let mut versions = Vec::new();
    const BASE_ARG: &str = "--base";
    const NEW_ARG: &str = "--new";

    if args.len() > 1 {
        if args.iter().any(|s| s == BASE_ARG) {
            versions.push("base");
        }
        if args.iter().any(|s| s == NEW_ARG) {
            versions.push("new");
        }
    }
    if versions.is_empty() {
        versions.push("new");
    }

    // Check if the target directory exists before starting
    if !std::path::Path::new("target/criterion").exists() {
        error!("'target/criterion' directory not found.");
        warn!("Please run 'cargo bench' first to generate benchmark data.");
        if let Ok(cwd) = env::current_dir() {
            warn!("Searched from CWD: {}", cwd.display());
        }
        return;
    }

    for version in versions {
        info!("--- Processing version: {} ---", version);
        match find_and_parse_benchmarks(version) {
            Ok(results) => {
                if results.is_empty() {
                    warn!("No benchmark results found for '{}'.", version);
                } else {
                    generate_table(version, results);
                }
            }
            Err(e) => {
                error!("Error processing benchmarks for version '{}': {}", version, e);
            }
        }
    }
}

/// Finds and parses all 'estimates.json' files for a specific version.
fn find_and_parse_benchmarks(
    version: &str,
) -> Result<HashMap<String, HashMap<String, f64>>, Box<dyn Error>> {
    let mut results: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let criterion_dir = "target/criterion";
    let version_path_segment = format!("/{}/", version); // e.g., "/new/"

    info!("Scanning for benchmarks in '{}' matching version '{}'", criterion_dir, version);

    // Use a filter_map to handle errors during directory traversal
    for entry_result in WalkDir::new(criterion_dir).into_iter() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(e) => {
                warn!("Error walking directory: {}", e);
                continue; // Skip this problematic entry
            }
        };

        debug!("Scanning file: {:?}", entry.path());

        // Check if the file is 'estimates.json'
        if entry.file_name().to_str() != Some("estimates.json") {
            continue;
        }

        let path = entry.path();
        debug!("Found potential estimates.json: {:?}", path);

        // Check if the path contains the version segment (e.g., "/new/")
        let path_str = match path.to_str() {
            Some(s) => s,
            None => {
                warn!("Skipping path with invalid UTF-8: {:?}", path);
                continue;
            }
        };

        if !path_str.contains(&version_path_segment) {
            debug!("Skipping (does not match version '{}'): {:?}", version, path);
            continue;
        }

        // --- Start of parsing logic ---
        // We found a matching file, now let's parse it
        info!("Processing benchmark file: {:?}", path);

        // Get benchmark name from '.../benchmark_name/version/estimates.json'
        let benchmark_name_str =
            if let Some(parent) = path.parent() // .../version
                && let Some(benchmark_dir) = parent.parent() // .../benchmark_name
                && let Some(benchmark_name) = benchmark_dir.file_name()
                && let Some(benchmark_name_str) = benchmark_name.to_str() {
                benchmark_name_str
            } else {
                warn!("Could not extract benchmark name from path: {:?}", path);
                continue;
            };

        // Open and parse the JSON file
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open file {:?}: {}", path, e);
                continue; // Skip this file
            }
        };
        let reader = BufReader::new(file);
        let benchmark: Benchmark = match serde_json::from_reader(reader) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to parse JSON file {:?}: {}", path, e);
                continue; // Skip this file
            }
        };

        // Split 'implementation-operation'
        let dash = benchmark_name_str.find("-").unwrap_or_default();
        let im: &str;
        let op: &str;
        if dash > 0 {
            im = &benchmark_name_str[..dash];
            op = &benchmark_name_str[dash + 1..];
        } else {
            im = benchmark_name_str;
            op = benchmark_name_str;
        }
        debug!("Parsed: op='{op}', impl='{im}', time={}",
            benchmark.mean.point_estimate);
        results.entry(op.to_string()).or_default()
            .insert(im.to_string(), benchmark.mean.point_estimate);
    }

    Ok(results)
}

/// Generates and prints a table from the collected results.
fn generate_table(version: &str, results: HashMap<String, HashMap<String, f64>>) {
    println!("\nResults for: {}", version);

    let mut operations: Vec<String> = results.keys().cloned().collect();
    operations.sort();

    let mut implementations: Vec<String> = results
        .values()
        .flat_map(|x| x.keys())
        .map(|s| s.to_string())
        .collect();
    implementations.sort();
    implementations.dedup();

    let mut table = Table::new();
    let mut header = vec![
        Cell::new("Benchmark"),
        Cell::new("Best Time"),
        Cell::new("Worst Time"),
    ];
    for impl_name in &implementations {
        header.push(Cell::new(impl_name));
    }
    table.set_header(header);

    for op in operations {
        let mut row = Row::new();
        row.add_cell(Cell::new(&op));

        let op_results = results.get(&op).unwrap(); // .unwrap() is safe here, we just got the key
        let best_time = op_results.values().cloned().fold(f64::INFINITY, f64::min);
        let worst_time = op_results.values().cloned().fold(f64::NEG_INFINITY, f64::max);

        row.add_cell(Cell::new(format_time(best_time)));
        row.add_cell(Cell::new(format_time(worst_time)));

        for impl_name in &implementations {
            if let Some(time) = op_results.get(impl_name) {
                let relative = time / best_time;
                row.add_cell(Cell::new(format!("{:.2}x", relative)));
            } else {
                row.add_cell(Cell::new("-"));
            }
        }
        table.add_row(row);
    }

    println!("{}", table);
}

fn format_time(time_ns: f64) -> String {
    if time_ns < 1_000.0 {
        format!("{:.2} ns", time_ns)
    } else if time_ns < 1_000_000.0 {
        format!("{:.2} µs", time_ns / 1_000.0)
    } else if time_ns < 1_000_000_000.0 {
        format!("{:.2} ms", time_ns / 1_000_000.0)
    } else {
        format!("{:.2} s", time_ns / 1_000_000_000.0)
    }
}
