use crate::model::{FpmSettings, Pool, PoolId, ProcessManager};
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const GENERATED_FILE: &str = "zz-fpm-lens.conf";
static ASSIGNMENT: OnceLock<Regex> = OnceLock::new();

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
    let unique_dirs: std::collections::BTreeSet<_> = dirs.iter().collect();
    for dir in unique_dirs {
        let entries = fs::read_dir(dir)
            .with_context(|| format!("could not read pool directory {}", dir.display()))?;
        let mut files = Vec::new();
        for entry in entries {
            let path = entry
                .with_context(|| format!("could not read an entry in {}", dir.display()))?
                .path();
            if path.extension().is_some_and(|value| value == "conf") {
                files.push(path);
            }
        }
        files.sort();
        for file in files {
            parse_file(dir, &file, &mut pools)?;
        }
    }
    pools
        .into_values()
        .filter(|pool| pool.settings.max_children.is_some())
        .map(|pool| {
            validate_settings(&pool)?;
            Ok(pool)
        })
        .collect()
}

fn validate_settings(pool: &Pool) -> Result<()> {
    let settings = &pool.settings;
    let max_children = settings.max_children.unwrap_or_default();
    if max_children == 0 {
        bail!("pool {} has a zero pm.max_children", pool.id.name);
    }
    if settings.pm == ProcessManager::Dynamic {
        for (name, value) in [
            ("pm.start_servers", settings.start_servers),
            ("pm.min_spare_servers", settings.min_spare_servers),
            ("pm.max_spare_servers", settings.max_spare_servers),
        ] {
            if value.is_some_and(|value| value == 0 || value > max_children) {
                bail!("pool {} has invalid {name}", pool.id.name);
            }
        }
        if settings
            .min_spare_servers
            .zip(settings.max_spare_servers)
            .is_some_and(|(min, max)| min > max)
        {
            bail!("pool {} has inverted spare-server bounds", pool.id.name);
        }
    }
    Ok(())
}

fn parse_file(
    dir: &Path,
    file: &Path,
    pools: &mut BTreeMap<(PathBuf, String), Pool>,
) -> Result<()> {
    let bytes = crate::fsutil::read_limited(file, 4 * 1024 * 1024, "pool configuration")?;
    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("pool configuration {} is not UTF-8", file.display()))?;
    let assignment = ASSIGNMENT
        .get_or_init(|| Regex::new(r"^([A-Za-z0-9_.]+)\s*=\s*([^;#]+)").expect("constant regex"));
    let mut section: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].trim();
            if name.is_empty() || name.chars().any(char::is_control) || name.contains(['[', ']']) {
                bail!("invalid pool section name in {}", file.display());
            }
            section = Some(name.to_owned());
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
        apply_setting(&mut pool.settings, key, value)
            .with_context(|| format!("invalid {key} in {}", file.display()))?;
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

fn apply_setting(s: &mut FpmSettings, key: &str, value: &str) -> Result<()> {
    match key {
        "pm" => {
            s.pm = match value {
                "static" => ProcessManager::Static,
                "dynamic" => ProcessManager::Dynamic,
                "ondemand" => ProcessManager::Ondemand,
                _ => bail!("unknown process manager {value:?}"),
            }
        }
        "pm.max_children" => s.max_children = Some(value.parse()?),
        "pm.max_requests" => s.max_requests = Some(value.parse()?),
        "pm.process_idle_timeout" => {
            s.process_idle_timeout_seconds = Some(seconds(value).context("invalid time value")?);
        }
        "request_terminate_timeout" => {
            s.request_terminate_timeout_seconds =
                Some(seconds(value).context("invalid time value")?);
        }
        "pm.start_servers" => s.start_servers = Some(value.parse()?),
        "pm.min_spare_servers" => s.min_spare_servers = Some(value.parse()?),
        "pm.max_spare_servers" => s.max_spare_servers = Some(value.parse()?),
        _ => {}
    }
    Ok(())
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
    #[test]
    fn includes_installed_fpm_lens_override() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("10.conf"), "[www]\npm.max_children=8\n").unwrap();
        fs::write(
            temp.path().join(GENERATED_FILE),
            "[www]\npm.max_children=14\n",
        )
        .unwrap();
        let pools = load_inventory(&[temp.path().to_path_buf()]).unwrap();
        assert_eq!(pools[0].settings.max_children, Some(14));
    }

    #[test]
    fn rejects_invalid_known_values() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("bad.conf"),
            "[www]\npm.max_children=lots\n",
        )
        .unwrap();
        assert!(load_inventory(&[temp.path().to_path_buf()]).is_err());
    }

    #[test]
    fn rejects_invalid_effective_pool_settings() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("bad.conf"),
            "[www]\npm=dynamic\npm.max_children=4\npm.min_spare_servers=5\n",
        )
        .unwrap();
        assert!(load_inventory(&[temp.path().to_path_buf()]).is_err());
    }
}
