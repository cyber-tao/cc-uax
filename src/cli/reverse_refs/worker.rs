use crate::cli::cache::CacheEntry;
use anyhow::{Result, anyhow};
use cc_uax::{MountMap, package_path_from_relative_with_mounts, referenced_packages_from_bytes};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub(crate) struct ScanInput<'a> {
    pub(crate) files: &'a [PathBuf],
    pub(crate) scan_dir: &'a Path,
    pub(crate) mount_map: &'a MountMap,
    pub(crate) self_rel_key: &'a str,
    pub(crate) self_pkg: &'a str,
    pub(crate) loaded: Option<&'a HashMap<String, CacheEntry>>,
    pub(crate) cache_enabled: bool,
    pub(crate) show_progress: bool,
}

pub(crate) struct ScanSummary {
    pub(crate) current: HashMap<String, CacheEntry>,
    pub(crate) referenced_by: Vec<String>,
    pub(crate) cached: usize,
    pub(crate) parsed: usize,
    pub(crate) skipped: usize,
}

pub(crate) fn scan_references(input: ScanInput<'_>) -> Result<ScanSummary> {
    let total = input.files.len();
    let show_progress = input.show_progress;
    let step = (total / 200).max(1);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let worker_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(total.max(1));
    let chunk_size = total.div_ceil(worker_count).max(1);

    let input_ref = &input;
    let partials: Vec<Partial> = std::thread::scope(|scope| {
        let handles: Vec<_> = input
            .files
            .chunks(chunk_size)
            .map(|chunk| {
                let done_ref = &done;
                scope.spawn(move || scan_chunk(chunk, input_ref, done_ref, step, total))
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

    let mut current = HashMap::new();
    let mut referenced_by = BTreeSet::new();
    let (mut cached, mut parsed, mut skipped) = (0usize, 0usize, 0usize);
    for p in partials {
        cached += p.cached;
        parsed += p.parsed;
        skipped += p.skipped;
        referenced_by.extend(p.referencers);
        current.extend(p.entries);
    }
    if show_progress && total > 0 {
        print_scan_progress(total, total);
        eprintln!();
    }

    Ok(ScanSummary {
        current,
        referenced_by: referenced_by.into_iter().collect(),
        cached,
        parsed,
        skipped,
    })
}

struct Partial {
    entries: Vec<(String, CacheEntry)>,
    referencers: Vec<String>,
    cached: usize,
    parsed: usize,
    skipped: usize,
}

fn scan_chunk(
    files: &[PathBuf],
    input: &ScanInput<'_>,
    done: &std::sync::atomic::AtomicUsize,
    step: usize,
    total: usize,
) -> Result<Partial> {
    let mut p = Partial {
        entries: Vec::new(),
        referencers: Vec::new(),
        cached: 0,
        parsed: 0,
        skipped: 0,
    };
    for path in files {
        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if input.show_progress && n.is_multiple_of(step) {
            print_scan_progress(n, total);
        }
        let rel = match path.strip_prefix(input.scan_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_key = rel.to_string_lossy().replace('\\', "/");
        let Some(package_path) = package_path_from_relative_with_mounts(&rel_key, input.mount_map)
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
        let cached_entry = input
            .loaded
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
            && !rel_key.eq_ignore_ascii_case(input.self_rel_key)
            && entry
                .refs
                .iter()
                .any(|r| r.eq_ignore_ascii_case(input.self_pkg))
        {
            p.referencers.push(package_path);
        }
        if input.cache_enabled {
            p.entries.push((rel_key, entry));
        }
    }
    Ok(p)
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
