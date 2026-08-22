use crate::model::{FpmSettings, Pool, PoolId, ProcessManager};
use anyhow::{Context, Result};
use regex::Regex;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const GENERATED_FILE: &str = "zz-fpm-lens.conf";

pub fn discover_pool_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in ["/etc/php", "/etc/opt/remi", "/opt/cpanel", "/usr/local/etc"] {
        walk_candidates(Path::new(root), 5, &mut dirs);
    }
    if Path::new("/etc/php-fpm.d").is_dir() {
        dirs.push(PathBuf::from("/etc/php-fpm.d"));
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

fn walk_candidates(path: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || !path.is_dir() {
        return;
    }
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    if matches!(name, "pool.d" | "php-fpm.d") {
        out.push(path.to_path_buf());
        return;
    }
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            walk_candidates(&entry.path(), depth - 1, out);
        }
    }
}

pub fn load_inventory(dirs: &[PathBuf]) -> Result<Vec<Pool>> {
    let mut pools: BTreeMap<(PathBuf, String), Pool> = BTreeMap::new();
    for dir in dirs {
        let mut files: Vec<_> = fs::read_dir(dir)
            .with_context(|| format!("could not read pool directory {}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|v| v == "conf")
                    && p.file_name().is_none_or(|v| v != GENERATED_FILE)
            })
            .collect();
        files.sort();
        for file in files {
            parse_file(dir, &file, &mut pools)?;
        }
    }
    Ok(pools
        .into_values()
        .filter(|p| p.settings.max_children.is_some())
        .collect())
}

fn parse_file(
    dir: &Path,
    file: &Path,
    pools: &mut BTreeMap<(PathBuf, String), Pool>,
) -> Result<()> {
    let text =
        fs::read_to_string(file).with_context(|| format!("could not read {}", file.display()))?;
    let assignment = Regex::new(r"^([A-Za-z0-9_.]+)\s*=\s*([^;#]+)").expect("constant regex");
    let mut section: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = Some(line[1..line.len() - 1].trim().to_owned());
            continue;
        }
        let Some(name) = section.as_ref().filter(|n| n.as_str() != "global") else {
            continue;
        };
        let Some(c) = assignment.captures(line) else {
            continue;
        };
        let key = c.get(1).expect("capture").as_str();
        let value = c.get(2).expect("capture").as_str().trim();
        let pool = pools
            .entry((dir.to_path_buf(), name.clone()))
            .or_insert_with(|| Pool {
                id: PoolId {
                    directory: dir.to_path_buf(),
                    name: name.clone(),
                },
                source_files: Vec::new(),
                settings: FpmSettings::default(),
            });
        if !pool.source_files.contains(&file.to_path_buf()) {
            pool.source_files.push(file.to_path_buf());
        }
        apply_setting(&mut pool.settings, key, value);
    }
    Ok(())
}

fn seconds(value: &str) -> Option<u32> {
    let value = value.trim();
    let split = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let number: u32 = value[..split].parse().ok()?;
    match value[split..].trim() {
        "" | "s" => Some(number),
        "m" => number.checked_mul(60),
        "h" => number.checked_mul(3600),
        "d" => number.checked_mul(86400),
        _ => None,
    }
}

fn apply_setting(s: &mut FpmSettings, key: &str, value: &str) {
    match key {
        "pm" => {
            s.pm = match value {
                "static" => ProcessManager::Static,
                "dynamic" => ProcessManager::Dynamic,
                "ondemand" => ProcessManager::Ondemand,
                _ => ProcessManager::Unknown,
            }
        }
        "pm.max_children" => s.max_children = value.parse().ok(),
        "pm.max_requests" => s.max_requests = value.parse().ok(),
        "pm.process_idle_timeout" => s.process_idle_timeout_seconds = seconds(value),
        "request_terminate_timeout" => s.request_terminate_timeout_seconds = seconds(value),
        "pm.start_servers" => s.start_servers = value.parse().ok(),
        "pm.min_spare_servers" => s.min_spare_servers = value.parse().ok(),
        "pm.max_spare_servers" => s.max_spare_servers = value.parse().ok(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_overlay_and_time_units() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("10.conf"),
            "[www]\npm=ondemand\npm.max_children=8\npm.process_idle_timeout=2m\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("20.conf"),
            "[www]\npm.max_children=12\nrequest_terminate_timeout=30s\n",
        )
        .unwrap();
        let pools = load_inventory(&[temp.path().to_path_buf()]).unwrap();
        assert_eq!(pools[0].settings.max_children, Some(12));
        assert_eq!(pools[0].settings.process_idle_timeout_seconds, Some(120));
        assert_eq!(
            pools[0].settings.request_terminate_timeout_seconds,
            Some(30)
        );
    }
}
