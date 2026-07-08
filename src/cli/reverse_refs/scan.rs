use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn collect_asset_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_asset_files_inner(dir, &mut files)?;
    files.sort_by_key(|path| path_key(path));
    Ok(files)
}

fn collect_asset_files_inner(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_asset_files_inner(&path, out)?;
        } else if file_type.is_file() && is_asset_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_asset_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("uasset") || e.eq_ignore_ascii_case("umap"))
        .unwrap_or(false)
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn asset_scan_is_sorted_and_filters_extensions() {
        let root = temp_dir("cc_uax_scan_sorted");
        fs::create_dir_all(root.join("B")).unwrap();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("B").join("two.umap"), []).unwrap();
        fs::write(root.join("a").join("one.uasset"), []).unwrap();
        fs::write(root.join("a").join("ignored.txt"), []).unwrap();

        let files = collect_asset_files(&root).unwrap();
        let rels: Vec<_> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(rels, vec!["a/one.uasset", "B/two.umap"]);

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{nanos}"))
    }
}
