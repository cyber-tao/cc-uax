use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::Path;

const CACHE_SCHEMA_VERSION: i64 = 1;

#[derive(Clone)]
pub struct CacheEntry {
    pub mtime: i64,
    pub size: i64,
    pub refs: Vec<String>,
}

pub struct RefCache {
    conn: Connection,
    loaded: HashMap<String, CacheEntry>,
}

impl RefCache {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open cache database: {}", path.display()))?;

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != CACHE_SCHEMA_VERSION {
            conn.execute("DROP TABLE IF EXISTS file_refs", [])?;
            conn.pragma_update(None, "user_version", CACHE_SCHEMA_VERSION)?;
        }
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file_refs (
                rel_path TEXT PRIMARY KEY,
                mtime    INTEGER NOT NULL,
                size     INTEGER NOT NULL,
                refs     TEXT NOT NULL
            )",
            [],
        )?;

        let mut loaded = HashMap::new();
        {
            let mut stmt = conn.prepare("SELECT rel_path, mtime, size, refs FROM file_refs")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                let (rel, mtime, size, refs) = row?;
                loaded.insert(
                    rel,
                    CacheEntry {
                        mtime,
                        size,
                        refs: split_refs(&refs),
                    },
                );
            }
        }

        Ok(RefCache { conn, loaded })
    }

    pub fn lookup(&self, rel: &str, mtime: i64, size: i64) -> Option<&[String]> {
        let entry = self.loaded.get(rel)?;
        if entry.mtime == mtime && entry.size == size {
            Some(&entry.refs)
        } else {
            None
        }
    }

    pub fn store(&mut self, current: &HashMap<String, CacheEntry>) -> Result<bool> {
        if !self.is_dirty(current) {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM file_refs", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO file_refs (rel_path, mtime, size, refs) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (rel, entry) in current {
                stmt.execute(params![
                    rel,
                    entry.mtime,
                    entry.size,
                    join_refs(&entry.refs)
                ])?;
            }
        }
        tx.commit()?;

        self.loaded = current.clone();
        Ok(true)
    }

    fn is_dirty(&self, current: &HashMap<String, CacheEntry>) -> bool {
        if current.len() != self.loaded.len() {
            return true;
        }
        current
            .iter()
            .any(|(rel, entry)| match self.loaded.get(rel) {
                Some(old) => old.mtime != entry.mtime || old.size != entry.size,
                None => true,
            })
    }
}

fn join_refs(refs: &[String]) -> String {
    refs.join("\n")
}

fn split_refs(s: &str) -> Vec<String> {
    if s.is_empty() {
        Vec::new()
    } else {
        s.split('\n').map(str::to_owned).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "cc-uax-cache-test-{}-{tag}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    #[test]
    fn refs_roundtrip() {
        assert!(split_refs("").is_empty());
        assert_eq!(split_refs("/Game/A"), vec!["/Game/A".to_string()]);
        let v = vec!["/Game/A".to_string(), "/Script/B".to_string()];
        assert_eq!(split_refs(&join_refs(&v)), v);
    }

    #[test]
    fn store_then_reload_respects_mtime_and_size() {
        let path = temp_db_path("hits");
        let _ = std::fs::remove_file(&path);

        let mut current = HashMap::new();
        current.insert(
            "Foo/BP_A.uasset".to_string(),
            CacheEntry {
                mtime: 111,
                size: 222,
                refs: vec!["/Game/Foo/Dep".to_string()],
            },
        );

        {
            let mut cache = RefCache::open(&path).unwrap();
            assert!(cache.lookup("Foo/BP_A.uasset", 111, 222).is_none());
            assert!(cache.store(&current).unwrap());

            assert!(!cache.store(&current).unwrap());
        }
        {
            let cache = RefCache::open(&path).unwrap();
            assert_eq!(
                cache.lookup("Foo/BP_A.uasset", 111, 222),
                Some(["/Game/Foo/Dep".to_string()].as_slice())
            );

            assert!(cache.lookup("Foo/BP_A.uasset", 111, 999).is_none());

            assert!(cache.lookup("Foo/BP_A.uasset", 333, 222).is_none());
        }

        let _ = std::fs::remove_file(&path);
    }
}
