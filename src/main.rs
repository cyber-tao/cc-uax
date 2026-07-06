use anyhow::{Context, Result, anyhow};
use cc_uax::package::{package_path_from_relative, referenced_packages_from_bytes};
use cc_uax::{OutputSections, Package};
use clap::Parser;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

mod cache;
use cache::{CacheEntry, RefCache};

#[derive(Parser)]
#[command(
    name = "cc-uax",
    version,
    about = "Parse UE5 Blueprint (.uasset) files into JSON",
    arg_required_else_help = true,
    after_help = r#"EXAMPLES:
  cc-uax asset.uasset                     Full JSON (summary + imports + exports)
  cc-uax asset.uasset -S logic            Graph nodes + pin connectivity only
  cc-uax asset.uasset -S debug            Summary + imports + full properties + layout
  cc-uax asset.uasset -S exports,pins     Pick sections explicitly
  cc-uax asset.uasset -c -o out.json      Write compact JSON to a file"#
)]
struct Args {
    #[arg(
        value_name = "INPUT",
        help = "Path to the UE5 Blueprint (.uasset) file to parse"
    )]
    input: PathBuf,

    #[arg(
        short,
        long,
        value_name = "FILE",
        help = "Write JSON to <FILE> instead of stdout"
    )]
    output: Option<PathBuf>,

    #[arg(short, long, help = "Output compact JSON instead of pretty-printed")]
    compact: bool,

    #[arg(
        short = 'S',
        long,
        value_name = "LIST",
        help = "Output sections to emit (comma-separated), or a preset. Sections: summary, imports, exports (alias: identity), pins, properties (props), layout, names, references (refs). Presets: logic (graph), debug, full (all; default)"
    )]
    sections: Option<String>,

    #[arg(
        short = 'd',
        long,
        value_name = "DIR",
        help = "Scan <DIR> recursively to also list assets that reference this file (with -S refs)"
    )]
    scan_dir: Option<PathBuf>,

    #[arg(
        short = 'm',
        long,
        value_name = "PREFIX",
        default_value = "/Game",
        help = "Mount prefix mapping <DIR> to package paths (default: /Game)"
    )]
    mount: String,

    #[arg(
        long,
        help = "Disable the on-disk reverse-reference cache (<DIR>/.cc-uax-cache.sqlite)"
    )]
    no_cache: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let data = fs::read(&args.input)
        .with_context(|| format!("Failed to read file: {}", args.input.display()))?;

    let package = Package::parse(&data)
        .with_context(|| format!("Failed to parse: {}", args.input.display()))?;

    let sections = match args.sections.as_deref() {
        Some(spec) => OutputSections::parse(spec)
            .with_context(|| format!("Invalid --sections value: '{spec}'"))?,
        None => OutputSections::full(),
    };

    let mut json = package.to_json(&data, &sections);

    if sections.references {
        if let Some(scan_dir) = args.scan_dir.as_deref() {
            let (self_pkg, referenced_by) =
                compute_referenced_by(&args.input, scan_dir, &args.mount, !args.no_cache)?;
            if let Value::Object(ref mut m) = json
                && let Some(Value::Object(refs)) = m.get_mut("references")
            {
                refs.insert("self".into(), json!(self_pkg));
                refs.insert("referenced_by".into(), json!(referenced_by));
            }
        }
    } else if args.scan_dir.is_some() {
        eprintln!("Note: --scan-dir only takes effect together with -S refs");
    }

    if let Value::Object(ref mut m) = json {
        m.insert("file".into(), json!(args.input.display().to_string()));
    }

    let text = if args.compact {
        serde_json::to_string(&json)?
    } else {
        serde_json::to_string_pretty(&json)?
    };

    match args.output {
        Some(path) => {
            fs::write(&path, &text)
                .with_context(|| format!("Failed to write file: {}", path.display()))?;
            eprintln!("Written to {}", path.display());
        }
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())?;
            out.write_all(b"\n")?;
        }
    }

    Ok(())
}

const CACHE_FILE_NAME: &str = ".cc-uax-cache.sqlite";

fn compute_referenced_by(
    input: &Path,
    scan_dir: &Path,
    mount: &str,
    use_cache: bool,
) -> Result<(String, Vec<String>)> {
    let input_abs = fs::canonicalize(input)
        .with_context(|| format!("Failed to locate input file: {}", input.display()))?;
    let scan_abs = fs::canonicalize(scan_dir)
        .with_context(|| format!("Failed to locate scan directory: {}", scan_dir.display()))?;

    let self_rel = input_abs.strip_prefix(&scan_abs).map_err(|_| {
        anyhow!(
            "Input file is not inside --scan-dir: {} is not under {}",
            input_abs.display(),
            scan_abs.display()
        )
    })?;

    let self_rel_key = self_rel.to_string_lossy().replace('\\', "/");
    let self_pkg = package_path_from_relative(&self_rel_key, mount);

    let mut files = Vec::new();
    collect_asset_files(&scan_abs, &mut files)
        .with_context(|| format!("Failed to scan directory: {}", scan_abs.display()))?;

    let total = files.len();
    let show_progress = std::io::stderr().is_terminal();

    let step = (total / 200).max(1);
    eprintln!("Found {total} assets under {}", scan_abs.display());

    let cache_path = scan_abs.join(CACHE_FILE_NAME);
    let mut cache = if use_cache {
        match RefCache::open(&cache_path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Cache disabled ({e:#})");
                None
            }
        }
    } else {
        None
    };

    let cache_enabled = cache.is_some();
    let loaded = cache.as_ref().map(|c| c.loaded_map());

    let done = std::sync::atomic::AtomicUsize::new(0);
    let worker_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(total.max(1));
    let chunk_size = total.div_ceil(worker_count).max(1);

    let self_pkg_ref = self_pkg.as_str();
    let self_rel_key_ref = self_rel_key.as_str();
    let scan_abs_ref = scan_abs.as_path();
    let done_ref = &done;

    struct Partial {
        entries: Vec<(String, CacheEntry)>,
        referencers: Vec<String>,
        cached: usize,
        parsed: usize,
        skipped: usize,
    }

    // The parse of each asset is independent, so fan the work out across the available
    // cores. Workers only read the immutable cache snapshot; the DB write happens after.
    let partials: Vec<Partial> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    let mut p = Partial {
                        entries: Vec::new(),
                        referencers: Vec::new(),
                        cached: 0,
                        parsed: 0,
                        skipped: 0,
                    };
                    for path in chunk {
                        let n = done_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if show_progress && n.is_multiple_of(step) {
                            print_scan_progress(n, total);
                        }
                        let rel = match path.strip_prefix(scan_abs_ref) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                        let rel_key = rel.to_string_lossy().replace('\\', "/");
                        let meta = match fs::metadata(path) {
                            Ok(m) => m,
                            Err(_) => {
                                p.skipped += 1;
                                continue;
                            }
                        };
                        let size = meta.len() as i64;
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_nanos() as i64)
                            .unwrap_or(0);
                        let cached_entry = loaded
                            .and_then(|m| m.get(&rel_key))
                            .filter(|e| e.is_fresh(mtime, size));
                        let entry = match cached_entry {
                            Some(e) => {
                                if e.parse_ok {
                                    p.cached += 1;
                                } else {
                                    p.skipped += 1;
                                }
                                e.clone()
                            }
                            None => match parse_referenced_packages(path) {
                                Some(refs) => {
                                    p.parsed += 1;
                                    CacheEntry {
                                        mtime,
                                        size,
                                        parse_ok: true,
                                        refs,
                                    }
                                }
                                None => {
                                    p.skipped += 1;
                                    CacheEntry {
                                        mtime,
                                        size,
                                        parse_ok: false,
                                        refs: Vec::new(),
                                    }
                                }
                            },
                        };
                        if entry.parse_ok
                            && !rel_key.eq_ignore_ascii_case(self_rel_key_ref)
                            && entry
                                .refs
                                .iter()
                                .any(|r| r.eq_ignore_ascii_case(self_pkg_ref))
                        {
                            p.referencers
                                .push(package_path_from_relative(&rel_key, mount));
                        }
                        if cache_enabled {
                            p.entries.push((rel_key, entry));
                        }
                    }
                    p
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut current: HashMap<String, CacheEntry> = HashMap::new();
    let mut referenced_by = BTreeSet::new();
    let (mut cached, mut parsed, mut skipped) = (0usize, 0usize, 0usize);
    for p in partials {
        cached += p.cached;
        parsed += p.parsed;
        skipped += p.skipped;
        for r in p.referencers {
            referenced_by.insert(r);
        }
        for (k, e) in p.entries {
            current.insert(k, e);
        }
    }
    if show_progress && total > 0 {
        print_scan_progress(total, total);
        eprintln!();
    }

    if let Some(c) = cache.as_mut() {
        match c.store(&current) {
            Ok(true) => eprintln!("Cache updated: {}", cache_path.display()),
            Ok(false) => {}
            Err(e) => eprintln!("Failed to write cache ({e:#})"),
        }
    }

    eprintln!(
        "Scanned {total} assets ({cached} cached, {parsed} parsed, {skipped} skipped), found {} referencer(s)",
        referenced_by.len()
    );
    Ok((self_pkg, referenced_by.into_iter().collect()))
}

fn parse_referenced_packages(path: &Path) -> Option<Vec<String>> {
    let data = fs::read(path).ok()?;
    referenced_packages_from_bytes(&data).ok()
}

fn print_scan_progress(done: usize, total: usize) {
    const BAR_WIDTH: usize = 24;
    let pct = if total == 0 { 100 } else { done * 100 / total };
    let filled = pct * BAR_WIDTH / 100;
    let bar = format!("{}{}", "#".repeat(filled), "-".repeat(BAR_WIDTH - filled));
    eprint!("\rScanning [{bar}] {pct:3}% ({done}/{total})");
    let _ = std::io::stderr().flush();
}

fn collect_asset_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_asset_files(&path, out)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("uasset") || e.eq_ignore_ascii_case("umap"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    Ok(())
}
