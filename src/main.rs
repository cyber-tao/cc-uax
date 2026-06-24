use anyhow::{Context, Result, anyhow};
use cc_uax::package::package_path_from_relative;
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
        help = "Output sections to emit (comma-separated), or a preset. Sections: summary, imports, exports, pins, properties, layout, names, references. Presets: logic, debug, full (default)"
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

    let mut current: HashMap<String, CacheEntry> = HashMap::new();

    let mut referenced_by = BTreeSet::new();
    let (mut cached, mut parsed, mut skipped) = (0usize, 0usize, 0usize);
    for (idx, path) in files.iter().enumerate() {
        if show_progress && idx % step == 0 {
            print_scan_progress(idx, total);
        }
        let rel = match path.strip_prefix(&scan_abs) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_key = rel.to_string_lossy().replace('\\', "/");

        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
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

        let cached_refs = cache
            .as_ref()
            .and_then(|c| c.lookup(&rel_key, mtime, size))
            .map(|refs| refs.to_vec());
        let refs = match cached_refs {
            Some(refs) => {
                cached += 1;
                refs
            }
            None => match parse_referenced_packages(path) {
                Some(refs) => {
                    parsed += 1;
                    refs
                }
                None => {
                    skipped += 1;
                    continue;
                }
            },
        };

        if cache.is_some() {
            current.insert(
                rel_key.clone(),
                CacheEntry {
                    mtime,
                    size,
                    refs: refs.clone(),
                },
            );
        }

        if !rel_key.eq_ignore_ascii_case(&self_rel_key)
            && refs.iter().any(|r| r.eq_ignore_ascii_case(&self_pkg))
        {
            referenced_by.insert(package_path_from_relative(&rel_key, mount));
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
    let pkg = Package::parse(&data).ok()?;
    Some(pkg.referenced_packages())
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
