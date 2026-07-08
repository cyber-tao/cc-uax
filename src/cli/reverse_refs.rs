use crate::cli::cache::{CacheEntry, RefCache};
use anyhow::{Context, Result, anyhow};
use cc_uax::{MountMap, package_path_from_relative_with_mounts, referenced_packages_from_bytes};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const CACHE_FILE_NAME: &str = ".cc-uax-cache.sqlite";

pub fn compute_referenced_by(
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
    let mount_map_ref = &mount_map;
    let done_ref = &done;

    struct Partial {
        entries: Vec<(String, CacheEntry)>,
        referencers: Vec<String>,
        cached: usize,
        parsed: usize,
        skipped: usize,
    }

    let partials: Vec<Partial> = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || -> Result<Partial> {
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
                        let Some(package_path) =
                            package_path_from_relative_with_mounts(&rel_key, mount_map_ref)
                        else {
                            p.skipped += 1;
                            continue;
                        };
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
                            p.referencers.push(package_path);
                        }
                        if cache_enabled {
                            p.entries.push((rel_key, entry));
                        }
                    }
                    Ok(p)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .map_err(|_| anyhow!("reverse-reference worker panicked"))?
            })
            .collect::<Result<Vec<_>>>()
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_panic_join_error_is_reported() {
        let result: Result<Vec<()>> = std::thread::scope(|scope| {
            let handle = scope.spawn(|| -> Result<()> {
                panic!("forced panic");
            });
            vec![handle]
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| anyhow!("reverse-reference worker panicked"))?
                })
                .collect()
        });

        let err = result.expect_err("panic should be converted to an error");
        assert!(err.to_string().contains("worker panicked"));
    }
}
