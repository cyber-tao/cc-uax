mod scan;
mod worker;

use crate::cli::cache::RefCache;
use anyhow::{Context, Result, anyhow};
use cc_uax::{MountMap, package_path_from_relative_with_mounts};
use std::fs;
use std::io::IsTerminal;
use std::path::Path;

const CACHE_FILE_NAME: &str = ".cc-uax-cache.sqlite";

pub(crate) fn compute_referenced_by(
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
    let mount_map = MountMap::parse(mount).map_err(anyhow::Error::msg)?;
    let self_pkg =
        package_path_from_relative_with_mounts(&self_rel_key, &mount_map).ok_or_else(|| {
            anyhow!(
                "Input file is not covered by --mount mapping: relative path '{}' under {}",
                self_rel_key,
                scan_abs.display()
            )
        })?;

    let files = scan::collect_asset_files(&scan_abs)
        .with_context(|| format!("Failed to scan directory: {}", scan_abs.display()))?;

    let total = files.len();
    let show_progress = std::io::stderr().is_terminal();
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

    let summary = worker::scan_references(worker::ScanInput {
        files: &files,
        scan_dir: &scan_abs,
        mount_map: &mount_map,
        self_rel_key: &self_rel_key,
        self_pkg: &self_pkg,
        loaded: cache.as_ref().map(|c| c.loaded_map()),
        cache_enabled: cache.is_some(),
        show_progress,
    })?;

    if let Some(c) = cache.as_mut() {
        match c.store(&summary.current) {
            Ok(true) => eprintln!("Cache updated: {}", cache_path.display()),
            Ok(false) => {}
            Err(e) => eprintln!("Failed to write cache ({e:#})"),
        }
    }

    eprintln!(
        "Scanned {total} assets ({} cached, {} parsed, {} skipped), found {} referencer(s)",
        summary.cached,
        summary.parsed,
        summary.skipped,
        summary.referenced_by.len()
    );
    Ok((self_pkg, summary.referenced_by))
}
