//! `pocketsmith snapshot` — create a consistent, integrity-checked backup.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, DatabaseName};
use serde_json::json;

use pocketsmith::db;

const DEFAULT_KEEP: usize = 28;
const PREFIX: &str = "pocketsmith-";
const SUFFIX: &str = ".db";

pub fn run(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        bail!("snapshot takes no arguments");
    }

    let db_path = db::path_from_env();
    let snapshot_dir = std::env::var("POCKETSMITH_SNAPSHOT_DIR")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_snapshot_dir(&db_path));
    let keep = std::env::var("POCKETSMITH_SNAPSHOT_KEEP")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_KEEP);

    let path = create_snapshot(&db_path, &snapshot_dir, keep)?;
    println!("{}", path.display());
    Ok(())
}

fn default_snapshot_dir(db_path: &str) -> PathBuf {
    Path::new(db_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("snapshots")
}

fn create_snapshot(db_path: &str, snapshot_dir: &Path, keep: usize) -> Result<PathBuf> {
    fs::create_dir_all(snapshot_dir)
        .with_context(|| format!("Failed to create {}", snapshot_dir.display()))?;

    let source = db::open_read_only(db_path)?;
    let timestamp: String = source.query_row(
        "SELECT strftime('%Y%m%dT%H%M%fZ', 'now')",
        [],
        |row| row.get(0),
    )?;
    let filename = format!("{PREFIX}{timestamp}{SUFFIX}");
    let destination = snapshot_dir.join(&filename);
    let temporary = snapshot_dir.join(format!(".{filename}.tmp"));

    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    source
        .backup(DatabaseName::Main, &temporary, None)
        .context("SQLite backup failed")?;

    let snapshot = Connection::open(&temporary)?;
    let check: String = snapshot.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if check != "ok" {
        bail!("snapshot integrity check failed: {check}");
    }
    drop(snapshot);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, &destination)?;

    let metadata = json!({
        "created_at": timestamp,
        "database": destination.file_name().and_then(|name| name.to_str()),
        "source": db_path,
    });
    let metadata_path = snapshot_dir.join("latest.json");
    let metadata_tmp = snapshot_dir.join(".latest.json.tmp");
    fs::write(&metadata_tmp, serde_json::to_vec_pretty(&metadata)?)?;
    fs::rename(metadata_tmp, metadata_path)?;

    prune(snapshot_dir, keep)?;
    Ok(destination)
}

fn prune(snapshot_dir: &Path, keep: usize) -> Result<()> {
    let mut snapshots = fs::read_dir(snapshot_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(PREFIX) && name.ends_with(SUFFIX))
        })
        .collect::<Vec<_>>();
    snapshots.sort();

    let remove_count = snapshots.len().saturating_sub(keep);
    for path in snapshots.into_iter().take(remove_count) {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("pocketsmith-snapshot-{unique}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn snapshot_is_consistent_and_prunes_old_files() {
        let root = temp_dir();
        let db_path = root.join("source.db");
        let snapshots = root.join("snapshots");
        let conn = pocketsmith::db::open_app_db_at(db_path.to_str().unwrap()).unwrap();
        conn.execute("INSERT INTO _operations (reason) VALUES ('test')", [])
            .unwrap();
        drop(conn);

        for name in ["pocketsmith-0001.db", "pocketsmith-0002.db"] {
            fs::write(snapshots.join(name), b"old").unwrap_or_else(|_| {
                fs::create_dir_all(&snapshots).unwrap();
                fs::write(snapshots.join(name), b"old").unwrap();
            });
        }

        let created = create_snapshot(db_path.to_str().unwrap(), &snapshots, 2).unwrap();
        assert!(created.exists());
        let check = Connection::open(created)
            .unwrap()
            .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
            .unwrap();
        assert_eq!(check, "ok");
        let retained = fs::read_dir(&snapshots)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".db"))
            .count();
        assert_eq!(retained, 2);
        assert!(snapshots.join("latest.json").exists());
    }
}
