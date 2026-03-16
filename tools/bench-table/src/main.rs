use comfy_table::{Cell, Color, Row, Table};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::collections::BTreeMap;
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

/// Represents a parsed benchmark result
#[derive(Debug, Clone)]
struct BenchResult {
    category: String,       // e.g., "list", "heap", "algo"
    operation: String,      // e.g., "append", "push", "dijkstra_dense"
    implementation: String, // e.g., "pielist", "vec", "binaryheap"
    size: Option<usize>,    // e.g., 100, 1000, 10000
    time_ns: f64,
}

/// Returns a description for a benchmark operation (used in markdown output)
fn get_benchmark_description(category: &str, operation: &str) -> Option<&'static str> {
    match (category.to_lowercase().as_str(), operation) {
        // List benchmarks (new naming)
        ("list", "append") => Some("Push N elements to the back. Vec wins due to cache locality."),
        ("list", "prepend") => Some("Push N elements to front. Linked lists O(1), Vec O(n²) total."),
        ("list", "iterate") => Some("Sum all elements. Vec wins with perfect cache locality."),
        ("list", "mid_modify") => Some("Insert+remove at middle. Linked lists O(1) modify vs Vec O(n)."),
        ("list", "multi_insert") => Some("Insert 100 elements at random positions. PieList O(n), Vec O(n²)."),
        ("list", "splice") => Some("Merge two lists at middle (includes O(n) position lookup)."),
        ("list", "splice_front") => Some("Merge at front (no traversal). PieList O(1) vs Vec O(n)."),
        ("list", "sort") => Some("Sort in place. Vec's pdqsort is highly optimized."),
        ("list", "random_access") => Some("Random index lookups. Vec O(1) vs linked list O(n)."),
        ("list", "pop_front") => Some("Pop all from front. PieList/VecDeque O(1), Vec O(n\u{00b2}) total."),
        ("list", "pop_back") => Some("Pop all from back. All O(1) per pop."),
        ("list", "retain") => Some("Filter ~50% in place. PieList unlinks, Vec shifts."),
        ("list", "drain") => Some("Consume all via drain iterator."),
        ("list", "cursor_traverse") => Some("Forward scan via cursor vs iterator."),
        ("list", "split") => Some("Split at midpoint. PieList O(1) split + O(n/2) seek."),

        // Pool benchmarks
        ("pool", "shared_lists") => Some("Create N lists, fill, clear. Tests pool reuse vs individual Vecs."),

        // Legacy naming (maps to list operations)
        ("legacy", "insert_remove_middle") => Some("Insert+remove at middle. Linked lists O(1) modify vs Vec O(n)."),
        ("legacy", "iter_sum") => Some("Sum all elements. Vec wins with perfect cache locality."),
        ("legacy", "push_back") => Some("Push N elements to the back. Vec wins due to cache locality."),
        ("legacy", "sort") => Some("Sort in place. Vec's pdqsort is highly optimized."),
        ("legacy", "splice_before_middle") => Some("Merge two lists at middle. PieList O(1) vs Vec O(n) copy."),

        // Heap benchmarks (new naming)
        ("heap", "push") => Some("Insert N elements. FibHeap O(1), BinaryHeap O(log n)."),
        ("heap", "pop") => Some("Extract all elements. BinaryHeap wins with simpler structure."),
        ("heap", "decrease_key") => Some("Update priorities. FibHeap O(1) - its key advantage!"),
        ("heap", "push_pop") => Some("Push N then pop N. Shows combined heap performance."),
        ("heap", "peek") => Some("Access minimum. All heaps O(1)."),
        ("heap", "drain") => Some("Pop all via drain/extract. BinaryHeap simpler pop."),
        ("heap", "mixed_workload") => Some("Interleaved push/decrease_key/pop (Dijkstra-like ratio)."),

        // Legacy heap naming
        ("other", "heap_push_sequential") => Some("Insert N elements. FibHeap O(1), BinaryHeap O(log n)."),
        ("other", "heap_pop_all_random") => Some("Extract all elements. BinaryHeap wins with simpler structure."),
        ("other", "heap_decrease_key_random") => Some("Update priorities. FibHeap O(1) - its key advantage!"),

        // Algorithm benchmarks
        ("algo", "dijkstra_dense") => Some("Shortest path on dense graph (n=100, m=5000)."),
        ("algo", "dijkstra_sparse") => Some("Shortest path on sparse grid (n=10k, m=20k)."),
        (_, op) if op.starts_with("Dijkstra") && op.contains("Dense") => {
            Some("Shortest path on dense graph. FibHeap benefits from many decrease_key ops.")
        }
        (_, op) if op.starts_with("Dijkstra") && op.contains("Sparse") => {
            Some("Shortest path on sparse graph. Fewer decrease_key ops favor simpler heaps.")
        }

        _ => None,
    }
}

/// Display mode options
#[derive(Debug, Clone)]
struct DisplayOptions {
    markdown_mode: bool,
    compact_mode: bool,      // Show only relative times, shorter column names
    vertical_mode: bool,     // Rows are implementations, columns are operations
    split_mode: bool,        // One small table per operation
    show_absolute: bool,     // Show absolute times (default: only in non-compact)
    show_bars: bool,         // Draw inline ASCII bar charts after relative times
    summary_mode: bool,      // One-line-per-category overview
    filter_impls: Vec<String>, // Only show these implementations
    filter_category: Option<String>, // Only show this category
}

impl Default for DisplayOptions {
    fn default() -> Self {
        Self {
            markdown_mode: false,
            compact_mode: false,  // Full view by default (split tables are narrow enough)
            vertical_mode: false,
            split_mode: true,     // Split mode by default (one table per operation)
            show_absolute: true,  // Show absolute times by default
            show_bars: false,
            summary_mode: false,
            filter_impls: Vec::new(),
            filter_category: None,
        }
    }
}

fn print_help() {
    println!("bench-table - Display Criterion benchmark results in a readable table

USAGE:
    bench-table [OPTIONS]

OPTIONS:
    --new           Show results from 'new' baseline (default)
    --base          Show results from 'base' baseline
    --markdown      Output in Markdown format
    --compact       Compact view: relative times only, no absolute times
    --full          Full view: show absolute times with relative (default)
    --vertical      Vertical layout: implementations as rows
    --split         Split: one mini-table per operation (default)
    --combined      Combined: single table with all operations as columns
    --bars          Draw inline ASCII bar charts proportional to slowdown
    --summary       One-line-per-category overview (headline wins/losses)
    --impl LIST     Filter to specific implementations (comma-separated)
                    Example: --impl pielist,vec,binaryheap
    --category CAT  Filter to specific category
                    Example: --category list
    --help, -h      Show this help message

REGRESSION DETECTION:
    When both 'base' and 'new' baselines exist, pass both to compare:
        bench-table --base --new
    Results will show \u{2191} (faster) or \u{2193} (slower) markers with percentage change.

EXAMPLES:
    bench-table                           # Default: split tables with full times
    bench-table --compact                 # Relative times only
    bench-table --bars                    # With ASCII bar charts
    bench-table --summary                 # Quick one-line-per-category overview
    bench-table --combined                # All operations in one wide table
    bench-table --impl pielist,vec        # Compare only pielist and vec
    bench-table --category heap           # Show only heap benchmarks
    bench-table --base --new              # Compare base vs new for regressions
    bench-table --vertical --impl pielist,binaryheap
");
}

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    // Check for help first
    if args.iter().any(|s| s == "--help" || s == "-h") {
        print_help();
        return;
    }

    let mut versions = Vec::new();
    let mut options = DisplayOptions::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => versions.push("base"),
            "--new" => versions.push("new"),
            "--markdown" => options.markdown_mode = true,
            "--compact" => options.compact_mode = true,
            "--full" => {
                options.compact_mode = false;
                options.show_absolute = true;
            }
            "--vertical" => options.vertical_mode = true,
            "--split" => options.split_mode = true,
            "--combined" => options.split_mode = false,
            "--bars" => options.show_bars = true,
            "--summary" => options.summary_mode = true,
            "--impl" => {
                i += 1;
                if i < args.len() {
                    options.filter_impls = args[i]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
            }
            "--category" => {
                i += 1;
                if i < args.len() {
                    options.filter_category = Some(args[i].clone());
                }
            }
            _ => {
                if args[i].starts_with('-') {
                    eprintln!("Unknown option: {}. Use --help for usage.", args[i]);
                }
            }
        }
        i += 1;
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

    // Collect results for regression comparison
    let mut all_version_results: Vec<(&str, Vec<BenchResult>)> = Vec::new();
    for version in &versions {
        info!("--- Processing version: {} ---", version);
        match find_and_parse_benchmarks(version) {
            Ok(results) => {
                if results.is_empty() {
                    warn!("No benchmark results found for '{}'.", version);
                } else {
                    all_version_results.push((version, results));
                }
            }
            Err(e) => {
                error!("Error processing benchmarks for version '{}': {}", version, e);
            }
        }
    }

    // Regression comparison: if we have both base and new, show markers
    if all_version_results.len() == 2
        && all_version_results[0].0 == "base"
        && all_version_results[1].0 == "new"
    {
        generate_regression_table(&all_version_results[0].1, &all_version_results[1].1, &options);
    } else if options.summary_mode {
        for (version, results) in &all_version_results {
            generate_summary(version, results, &options);
        }
    } else {
        for (version, results) in all_version_results {
            generate_tables(version, results, &options);
        }
    }
}

/// Finds and parses all 'estimates.json' files for a specific version.
fn find_and_parse_benchmarks(
    version: &str,
) -> Result<Vec<BenchResult>, Box<dyn Error>> {
    let mut results: Vec<BenchResult> = Vec::new();
    let criterion_dir = "target/criterion";
    let version_path_segment = format!("/{}/", version); // e.g., "/new/"

    info!("Scanning for benchmarks in '{}' matching version '{}'", criterion_dir, version);

    for entry_result in WalkDir::new(criterion_dir).into_iter() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(e) => {
                warn!("Error walking directory: {}", e);
                continue;
            }
        };

        // Check if the file is 'estimates.json'
        if entry.file_name().to_str() != Some("estimates.json") {
            continue;
        }

        let path = entry.path();

        let path_str = match path.to_str() {
            Some(s) => s,
            None => {
                warn!("Skipping path with invalid UTF-8: {:?}", path);
                continue;
            }
        };

        if !path_str.contains(&version_path_segment) {
            continue;
        }

        debug!("Processing benchmark file: {:?}", path);

        // Navigate up from estimates.json to find benchmark structure
        // Path: .../benchmark_group/benchmark_name/[size]/version/estimates.json
        let version_dir = match path.parent() {
            Some(d) => d,
            None => continue,
        };

        // Try to parse the path structure
        // Look for the pattern by going up from version dir
        let mut current = version_dir;
        let mut path_parts: Vec<&str> = Vec::new();

        while let Some(parent) = current.parent() {
            if let Some(name) = current.file_name().and_then(|n| n.to_str()) {
                if name == "criterion" {
                    break;
                }
                if name != version {
                    path_parts.push(name);
                }
            }
            current = parent;
        }
        path_parts.reverse();

        debug!("Path parts: {:?}", path_parts);

        // Parse the benchmark structure
        let bench_result = if path_parts.len() >= 3 {
            // New format: category/operation, implementation, [size]
            let group_name = path_parts[0]; // e.g., "list/append"
            let impl_name = path_parts[1]; // e.g., "pielist"
            let size: Option<usize> = path_parts.get(2).and_then(|s| s.parse().ok());

            if group_name.contains('/') {
                let group_parts: Vec<&str> = group_name.split('/').collect();
                Some(BenchResult {
                    category: group_parts[0].to_string(),
                    operation: group_parts[1..].join("/"),
                    implementation: impl_name.to_string(),
                    size,
                    time_ns: 0.0,
                })
            } else {
                // Algo benchmarks: "algo/dijkstra_dense", "petgraph_binaryheap"
                Some(BenchResult {
                    category: "other".to_string(),
                    operation: group_name.to_string(),
                    implementation: impl_name.to_string(),
                    size,
                    time_ns: 0.0,
                })
            }
        } else if path_parts.len() == 2 {
            // Could be old format or algo without size
            let name = path_parts[0];
            let sub = path_parts[1];

            if name.contains('/') {
                let parts: Vec<&str> = name.split('/').collect();
                Some(BenchResult {
                    category: parts[0].to_string(),
                    operation: parts[1..].join("/"),
                    implementation: sub.to_string(),
                    size: None,
                    time_ns: 0.0,
                })
            } else {
                // Old style: impl-operation
                if let Some(dash_pos) = name.find('-') {
                    Some(BenchResult {
                        category: "legacy".to_string(),
                        operation: name[dash_pos + 1..].to_string(),
                        implementation: name[..dash_pos].to_string(),
                        size: None,
                        time_ns: 0.0,
                    })
                } else {
                    Some(BenchResult {
                        category: "other".to_string(),
                        operation: name.to_string(),
                        implementation: sub.to_string(),
                        size: None,
                        time_ns: 0.0,
                    })
                }
            }
        } else if path_parts.len() == 1 {
            // Single name - old format
            let name = path_parts[0];
            if let Some(dash_pos) = name.find('-') {
                Some(BenchResult {
                    category: "legacy".to_string(),
                    operation: name[dash_pos + 1..].to_string(),
                    implementation: name[..dash_pos].to_string(),
                    size: None,
                    time_ns: 0.0,
                })
            } else {
                None
            }
        } else {
            None
        };

        let mut bench_result = match bench_result {
            Some(r) => r,
            None => {
                warn!("Could not parse benchmark structure from: {:?}", path);
                continue;
            }
        };

        // Parse the JSON file
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) => {
                error!("Failed to open file {:?}: {}", path, e);
                continue;
            }
        };
        let reader = BufReader::new(file);
        let benchmark: Benchmark = match serde_json::from_reader(reader) {
            Ok(b) => b,
            Err(e) => {
                error!("Failed to parse JSON file {:?}: {}", path, e);
                continue;
            }
        };

        bench_result.time_ns = benchmark.mean.point_estimate;

        debug!("Parsed: {:?}", bench_result);
        results.push(bench_result);
    }

    Ok(results)
}

/// Groups benchmark results by category and operation for organized display.
fn group_results(results: Vec<BenchResult>) -> BTreeMap<String, BTreeMap<String, Vec<BenchResult>>> {
    let mut grouped: BTreeMap<String, BTreeMap<String, Vec<BenchResult>>> = BTreeMap::new();

    for result in results {
        grouped
            .entry(result.category.clone())
            .or_default()
            .entry(result.operation.clone())
            .or_default()
            .push(result);
    }

    grouped
}

/// Filters implementations based on options
fn filter_implementations(implementations: &[String], options: &DisplayOptions) -> Vec<String> {
    if options.filter_impls.is_empty() {
        implementations.to_vec()
    } else {
        implementations
            .iter()
            .filter(|impl_name| {
                options.filter_impls.iter().any(|f| impl_name.contains(f))
            })
            .cloned()
            .collect()
    }
}

/// Generates and prints tables organized by category.
fn generate_tables(version: &str, results: Vec<BenchResult>, options: &DisplayOptions) {
    let grouped = group_results(results);

    println!("\n# Benchmark Results ({})\n", version);

    for (category, operations) in grouped {
        // Apply category filter
        if let Some(ref filter_cat) = options.filter_category {
            if !category.to_lowercase().contains(&filter_cat.to_lowercase()) {
                continue;
            }
        }

        println!("\n## {}\n", category.to_uppercase());

        // Collect all implementations across all operations in this category
        let mut all_implementations: Vec<String> = operations
            .values()
            .flatten()
            .map(|r| r.implementation.clone())
            .collect();
        all_implementations.sort();
        all_implementations.dedup();

        // Apply implementation filter
        let filtered_implementations = filter_implementations(&all_implementations, options);

        if filtered_implementations.is_empty() {
            println!("  (no matching implementations)\n");
            continue;
        }

        // Collect all sizes
        let mut all_sizes: Vec<Option<usize>> = operations
            .values()
            .flatten()
            .map(|r| r.size)
            .collect();
        all_sizes.sort_by(|a, b| a.unwrap_or(0).cmp(&b.unwrap_or(0)));
        all_sizes.dedup();

        let has_sizes = all_sizes.iter().any(|s| s.is_some());

        if options.vertical_mode {
            // Vertical layout: one table per category, implementations as rows
            generate_vertical_table(&operations, &filtered_implementations, options);
        } else if options.split_mode {
            // Split mode: one mini-table per operation (narrow, easy to scan)
            for (operation, results) in &operations {
                generate_split_table(&category, operation, results, &filtered_implementations, options);
            }
        } else if has_sizes {
            // Per-operation tables with size breakdown
            for (operation, results) in &operations {
                generate_operation_table(operation, results, &filtered_implementations, options);
            }
        } else {
            // Single table for all operations in category
            generate_category_table(&operations, &filtered_implementations, options);
        }
    }
}

/// Generates a table for a single operation with size breakdown.
fn generate_operation_table(
    operation: &str,
    results: &[BenchResult],
    implementations: &[String],
    options: &DisplayOptions,
) {
    println!("### {}\n", operation);

    // Group by size
    let mut by_size: BTreeMap<usize, Vec<&BenchResult>> = BTreeMap::new();
    for r in results {
        if let Some(size) = r.size {
            by_size.entry(size).or_default().push(r);
        }
    }

    if options.markdown_mode {
        generate_operation_table_markdown(&by_size, implementations, options);
    } else {
        generate_operation_table_terminal(&by_size, implementations, options);
    }
}

fn generate_operation_table_markdown(
    by_size: &BTreeMap<usize, Vec<&BenchResult>>,
    implementations: &[String],
    options: &DisplayOptions,
) {
    let mut header = String::from("| Size |");
    let mut separator = String::from("|---:|");
    for impl_name in implementations {
        let short_name = shorten_impl_name(impl_name);
        header.push_str(&format!(" {} |", short_name));
        separator.push_str("---:|");
    }
    println!("{}", header);
    println!("{}", separator);

    for (size, size_results) in by_size {
        let best_time = size_results
            .iter()
            .map(|r| r.time_ns)
            .fold(f64::INFINITY, f64::min);

        let mut row = format!("| {} |", format_size(*size));
        for impl_name in implementations {
            if let Some(result) = size_results.iter().find(|r| &r.implementation == impl_name) {
                let relative = result.time_ns / best_time;
                if options.compact_mode {
                    if relative <= 1.05 {
                        row.push_str(" **1.00x** |");
                    } else {
                        row.push_str(&format!(" {:.2}x |", relative));
                    }
                } else {
                    let time_str = format_time(result.time_ns);
                    if relative <= 1.05 {
                        row.push_str(&format!(" **{}** (1.00x) |", time_str));
                    } else {
                        row.push_str(&format!(" {} ({:.2}x) |", time_str, relative));
                    }
                }
            } else {
                row.push_str(" - |");
            }
        }
        println!("{}", row);
    }
    println!();
}

fn generate_operation_table_terminal(
    by_size: &BTreeMap<usize, Vec<&BenchResult>>,
    implementations: &[String],
    options: &DisplayOptions,
) {
    let mut table = Table::new();
    let mut header = vec![Cell::new("Size")];
    for impl_name in implementations {
        let display_name = shorten_impl_name(impl_name);
        header.push(Cell::new(display_name));
    }
    table.set_header(header);

    for (size, size_results) in by_size {
        let best_time = size_results
            .iter()
            .map(|r| r.time_ns)
            .fold(f64::INFINITY, f64::min);

        let mut row = Row::new();
        row.add_cell(Cell::new(format_size(*size)));

        for impl_name in implementations {
            if let Some(result) = size_results.iter().find(|r| &r.implementation == impl_name) {
                let relative = result.time_ns / best_time;
                let cell_content = if options.compact_mode {
                    format!("{:.2}x", relative)
                } else {
                    let time_str = format_time(result.time_ns);
                    format!("{} ({:.2}x)", time_str, relative)
                };

                let mut cell = Cell::new(cell_content);
                if relative <= 1.05 {
                    cell = cell.fg(Color::Green);
                } else if relative >= 2.0 {
                    cell = cell.fg(Color::Red);
                }
                row.add_cell(cell);
            } else {
                row.add_cell(Cell::new("-"));
            }
        }
        table.add_row(row);
    }

    println!("{}\n", table);
}

/// Generates a table for all operations in a category (no size breakdown).
fn generate_category_table(
    operations: &BTreeMap<String, Vec<BenchResult>>,
    implementations: &[String],
    options: &DisplayOptions,
) {
    if options.markdown_mode {
        generate_category_table_markdown(operations, implementations, options);
    } else {
        generate_category_table_terminal(operations, implementations, options);
    }
}

fn generate_category_table_markdown(
    operations: &BTreeMap<String, Vec<BenchResult>>,
    implementations: &[String],
    options: &DisplayOptions,
) {
    let mut header = String::from("| Operation |");
    let mut separator = String::from("|:---|");
    for impl_name in implementations {
        let short_name = shorten_impl_name(impl_name);
        header.push_str(&format!(" {} |", short_name));
        separator.push_str("---:|");
    }
    println!("{}", header);
    println!("{}", separator);

    for (operation, results) in operations {
        let best_time = results
            .iter()
            .map(|r| r.time_ns)
            .fold(f64::INFINITY, f64::min);

        let mut row = format!("| {} |", operation);
        for impl_name in implementations {
            if let Some(result) = results.iter().find(|r| &r.implementation == impl_name) {
                let relative = result.time_ns / best_time;
                if options.compact_mode {
                    if relative <= 1.05 {
                        row.push_str(" **1.00x** |");
                    } else {
                        row.push_str(&format!(" {:.2}x |", relative));
                    }
                } else {
                    let time_str = format_time(result.time_ns);
                    if relative <= 1.05 {
                        row.push_str(&format!(" **{}** (1.00x) |", time_str));
                    } else {
                        row.push_str(&format!(" {} ({:.2}x) |", time_str, relative));
                    }
                }
            } else {
                row.push_str(" - |");
            }
        }
        println!("{}", row);
    }
    println!();
}

fn generate_category_table_terminal(
    operations: &BTreeMap<String, Vec<BenchResult>>,
    implementations: &[String],
    options: &DisplayOptions,
) {
    let mut table = Table::new();
    let mut header = vec![Cell::new("Operation")];
    for impl_name in implementations {
        let display_name = shorten_impl_name(impl_name);
        header.push(Cell::new(display_name));
    }
    table.set_header(header);

    for (operation, results) in operations {
        let best_time = results
            .iter()
            .map(|r| r.time_ns)
            .fold(f64::INFINITY, f64::min);

        let mut row = Row::new();
        row.add_cell(Cell::new(operation));

        for impl_name in implementations {
            if let Some(result) = results.iter().find(|r| &r.implementation == impl_name) {
                let relative = result.time_ns / best_time;
                let cell_content = if options.compact_mode {
                    format!("{:.2}x", relative)
                } else {
                    let time_str = format_time(result.time_ns);
                    format!("{} ({:.2}x)", time_str, relative)
                };

                let mut cell = Cell::new(cell_content);
                if relative <= 1.05 {
                    cell = cell.fg(Color::Green);
                } else if relative >= 2.0 {
                    cell = cell.fg(Color::Red);
                }
                row.add_cell(cell);
            } else {
                row.add_cell(Cell::new("-"));
            }
        }
        table.add_row(row);
    }

    println!("{}\n", table);
}

/// Generates a vertical table (implementations as rows, operations as columns)
fn generate_vertical_table(
    operations: &BTreeMap<String, Vec<BenchResult>>,
    implementations: &[String],
    options: &DisplayOptions,
) {
    let mut table = Table::new();

    // Header: Implementation | Op1 | Op2 | ...
    let mut header = vec![Cell::new("Implementation")];
    for op_name in operations.keys() {
        header.push(Cell::new(op_name));
    }
    table.set_header(header);

    // One row per implementation
    for impl_name in implementations {
        let mut row = Row::new();
        let display_name = shorten_impl_name(impl_name);
        row.add_cell(Cell::new(display_name));

        for (_, op_results) in operations {
            // Find the best time across all implementations for this operation
            let best_time = op_results
                .iter()
                .map(|r| r.time_ns)
                .fold(f64::INFINITY, f64::min);

            if let Some(result) = op_results.iter().find(|r| &r.implementation == impl_name) {
                let relative = result.time_ns / best_time;
                let cell_content = if options.compact_mode {
                    format!("{:.2}x", relative)
                } else {
                    let time_str = format_time(result.time_ns);
                    format!("{} ({:.2}x)", time_str, relative)
                };

                let mut cell = Cell::new(cell_content);
                if relative <= 1.05 {
                    cell = cell.fg(Color::Green);
                } else if relative >= 2.0 {
                    cell = cell.fg(Color::Red);
                }
                row.add_cell(cell);
            } else {
                row.add_cell(Cell::new("-"));
            }
        }
        table.add_row(row);
    }

    println!("{}\n", table);
}

/// Generates a compact mini-table for a single operation (split mode).
/// This produces narrow tables with Implementation and Time columns.
fn generate_split_table(
    category: &str,
    operation: &str,
    results: &[BenchResult],
    implementations: &[String],
    options: &DisplayOptions,
) {
    // Filter to only implementations that have results for this operation
    let relevant_impls: Vec<_> = implementations
        .iter()
        .filter(|impl_name| results.iter().any(|r| &r.implementation == *impl_name))
        .collect();

    if relevant_impls.is_empty() {
        return;
    }

    let best_time = results
        .iter()
        .filter(|r| implementations.contains(&r.implementation))
        .map(|r| r.time_ns)
        .fold(f64::INFINITY, f64::min);

    if options.markdown_mode {
        println!("### {}", operation);
        // Add description if available
        if let Some(desc) = get_benchmark_description(category, operation) {
            println!("\n_{}_\n", desc);
        } else {
            println!();
        }

        if options.compact_mode {
            println!("| Implementation | Relative |");
            println!("|:---|---:|");
        } else {
            println!("| Implementation | Time | Relative |");
            println!("|:---|---:|---:|");
        }

        for impl_name in &relevant_impls {
            if let Some(result) = results.iter().find(|r| &r.implementation == *impl_name) {
                let relative = result.time_ns / best_time;
                let short_name = shorten_impl_name(impl_name);
                if options.compact_mode {
                    if relative <= 1.05 {
                        println!("| {} | **1.00x** |", short_name);
                    } else {
                        println!("| {} | {:.2}x |", short_name, relative);
                    }
                } else {
                    let time_str = format_time(result.time_ns);
                    if relative <= 1.05 {
                        println!("| {} | **{}** | **1.00x** |", short_name, time_str);
                    } else {
                        println!("| {} | {} | {:.2}x |", short_name, time_str, relative);
                    }
                }
            }
        }
        println!();
    } else {
        let mut table = Table::new();

        // Print operation name in cyan for visibility
        println!();
        println!("  \x1b[36m{}\x1b[0m", operation);

        if options.compact_mode {
            if options.show_bars {
                table.set_header(vec![Cell::new("Impl"), Cell::new("Rel"), Cell::new("Bar")]);
            } else {
                table.set_header(vec![Cell::new("Impl"), Cell::new("Rel")]);
            }
        } else if options.show_bars {
            table.set_header(vec![Cell::new("Impl"), Cell::new("Time"), Cell::new("Rel"), Cell::new("Bar")]);
        } else {
            table.set_header(vec![Cell::new("Impl"), Cell::new("Time"), Cell::new("Rel")]);
        }

        for impl_name in &relevant_impls {
            if let Some(result) = results.iter().find(|r| &r.implementation == *impl_name) {
                let relative = result.time_ns / best_time;
                let short_name = shorten_impl_name(impl_name);

                let mut row = Row::new();
                row.add_cell(Cell::new(&short_name));

                if !options.compact_mode {
                    row.add_cell(Cell::new(format_time(result.time_ns)));
                }

                let rel_str = format!("{:.2}x", relative);
                let mut rel_cell = Cell::new(&rel_str);
                if relative <= 1.05 {
                    rel_cell = rel_cell.fg(Color::Green);
                } else if relative >= 2.0 {
                    rel_cell = rel_cell.fg(Color::Red);
                }
                row.add_cell(rel_cell);

                if options.show_bars {
                    let bar = format_bar(relative, 16);
                    let mut bar_cell = Cell::new(&bar);
                    if relative <= 1.05 {
                        bar_cell = bar_cell.fg(Color::Green);
                    } else if relative >= 2.0 {
                        bar_cell = bar_cell.fg(Color::Red);
                    }
                    row.add_cell(bar_cell);
                }

                table.add_row(row);
            }
        }
        println!("{}", table);
    }
}

/// Shorten implementation names for compact display
fn shorten_impl_name(name: &str) -> String {
    // With split view, tables are narrow enough - no abbreviations needed
    name.to_string()
}

fn format_time(time_ns: f64) -> String {
    if time_ns < 1_000.0 {
        format!("{:.1}ns", time_ns)
    } else if time_ns < 1_000_000.0 {
        format!("{:.1}µs", time_ns / 1_000.0)
    } else if time_ns < 1_000_000_000.0 {
        format!("{:.1}ms", time_ns / 1_000_000.0)
    } else {
        format!("{:.1}s", time_ns / 1_000_000_000.0)
    }
}

fn format_size(size: usize) -> String {
    if size >= 1_000_000 {
        format!("{}M", size / 1_000_000)
    } else if size >= 1_000 {
        format!("{}k", size / 1_000)
    } else {
        format!("{}", size)
    }
}

/// Draws an ASCII bar proportional to the relative slowdown.
/// 1.00x gets a full bar (max_width chars), higher values get shorter bars
/// (inverted so fastest = longest bar for visual impact).
fn format_bar(relative: f64, max_width: usize) -> String {
    // Clamp to max 10x for display purposes
    let clamped = relative.min(10.0);
    // Invert: 1.0x -> full bar, 10.0x -> tiny bar
    let fraction = 1.0 / clamped;
    let width = ((fraction * max_width as f64).round() as usize).max(1);
    "\u{2588}".repeat(width)
}

/// Generates a one-line-per-category summary showing headline wins/losses.
fn generate_summary(
    version: &str,
    results: &[BenchResult],
    options: &DisplayOptions,
) {
    let grouped = group_results(results.to_vec());
    println!("\n# Summary ({})\n", version);
    if options.markdown_mode {
        println!("| Category | Operation | Winner | Margin | Notes |");
        println!("|:---|:---|:---|---:|:---|");
    }
    for (category, operations) in &grouped {
        if let Some(ref filter_cat) = options.filter_category {
            if !category.to_lowercase().contains(&filter_cat.to_lowercase()) {
                continue;
            }
        }
        for (operation, op_results) in operations {
            if op_results.is_empty() {
                continue;
            }
            let best = op_results.iter().min_by(|a, b| a.time_ns.partial_cmp(&b.time_ns).unwrap());
            let worst = op_results.iter().max_by(|a, b| a.time_ns.partial_cmp(&b.time_ns).unwrap());
            if let (Some(best), Some(worst)) = (best, worst) {
                let margin = worst.time_ns / best.time_ns;
                let desc = get_benchmark_description(category, operation).unwrap_or("");
                if options.markdown_mode {
                    println!("| {} | {} | {} | {:.1}x | {} |",
                        category, operation, best.implementation, margin, desc);
                } else {
                    let margin_str = format!("{:.1}x", margin);
                    let bar = if margin >= 2.0 {
                        format!(" {}", format_bar(1.0, 16))
                    } else {
                        String::new()
                    };
                    println!("  {}/{}: \x1b[32m{}\x1b[0m wins by {}{}",
                        category, operation, best.implementation, margin_str, bar);
                }
            }
        }
    }
    println!();
}

/// Key for matching benchmarks across base/new versions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct BenchKey {
    category: String,
    operation: String,
    implementation: String,
    size: Option<usize>,
}

/// Generates a regression comparison table between base and new results.
fn generate_regression_table(
    base_results: &[BenchResult],
    new_results: &[BenchResult],
    options: &DisplayOptions,
) {
    use std::collections::BTreeMap;
    let mut base_map: BTreeMap<BenchKey, f64> = BTreeMap::new();
    for r in base_results {
        let key = BenchKey {
            category: r.category.clone(),
            operation: r.operation.clone(),
            implementation: r.implementation.clone(),
            size: r.size,
        };
        base_map.insert(key, r.time_ns);
    }
    let mut new_map: BTreeMap<BenchKey, f64> = BTreeMap::new();
    for r in new_results {
        let key = BenchKey {
            category: r.category.clone(),
            operation: r.operation.clone(),
            implementation: r.implementation.clone(),
            size: r.size,
        };
        new_map.insert(key, r.time_ns);
    }
    println!("\n# Regression Report (base → new)\n");
    if options.markdown_mode {
        println!("| Benchmark | Impl | Size | Base | New | Change |");
        println!("|:---|:---|---:|---:|---:|:---|");
    } else {
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Benchmark"),
            Cell::new("Impl"),
            Cell::new("Size"),
            Cell::new("Base"),
            Cell::new("New"),
            Cell::new("Change"),
        ]);
        let mut has_rows = false;
        // Iterate over all keys present in either base or new
        let mut all_keys: Vec<&BenchKey> = base_map.keys().chain(new_map.keys()).collect();
        all_keys.sort();
        all_keys.dedup();
        for key in &all_keys {
            if let Some(ref filter_cat) = options.filter_category {
                if !key.category.to_lowercase().contains(&filter_cat.to_lowercase()) {
                    continue;
                }
            }
            let base_ns = base_map.get(key);
            let new_ns = new_map.get(key);
            if let (Some(&base), Some(&new)) = (base_ns, new_ns) {
                let pct_change = ((new - base) / base) * 100.0;
                let marker = if pct_change < -5.0 {
                    format!("\x1b[32m\u{2191} {:.1}% faster\x1b[0m", -pct_change)
                } else if pct_change > 5.0 {
                    format!("\x1b[31m\u{2193} {:.1}% slower\x1b[0m", pct_change)
                } else {
                    format!("\u{2194} {:.1}%", pct_change)
                };
                let size_str = key.size.map_or("-".to_string(), |s| format_size(s));
                let mut row = Row::new();
                row.add_cell(Cell::new(format!("{}/{}", key.category, key.operation)));
                row.add_cell(Cell::new(&key.implementation));
                row.add_cell(Cell::new(&size_str));
                row.add_cell(Cell::new(format_time(base)));
                row.add_cell(Cell::new(format_time(new)));
                row.add_cell(Cell::new(&marker));
                table.add_row(row);
                has_rows = true;
            }
        }
        if has_rows {
            println!("{}\n", table);
        } else {
            println!("  (no matching benchmarks found in both base and new)\n");
        }
        return;
    }
    // Markdown path
    let mut all_keys: Vec<&BenchKey> = base_map.keys().chain(new_map.keys()).collect();
    all_keys.sort();
    all_keys.dedup();
    for key in &all_keys {
        if let Some(ref filter_cat) = options.filter_category {
            if !key.category.to_lowercase().contains(&filter_cat.to_lowercase()) {
                continue;
            }
        }
        let base_ns = base_map.get(key);
        let new_ns = new_map.get(key);
        if let (Some(&base), Some(&new)) = (base_ns, new_ns) {
            let pct_change = ((new - base) / base) * 100.0;
            let marker = if pct_change < -5.0 {
                format!("\u{2191} {:.1}% faster", -pct_change)
            } else if pct_change > 5.0 {
                format!("\u{2193} {:.1}% slower", pct_change)
            } else {
                format!("\u{2194} {:.1}%", pct_change)
            };
            let size_str = key.size.map_or("-".to_string(), |s| format_size(s));
            println!("| {}/{} | {} | {} | {} | {} | {} |",
                key.category, key.operation,
                key.implementation, size_str,
                format_time(base), format_time(new), marker);
        }
    }
    println!();
}
