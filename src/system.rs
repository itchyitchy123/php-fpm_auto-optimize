use anyhow::{Context, Result, bail};
use std::{fs, path::PathBuf};

pub fn detect_memory_mb() -> Result<u64> {
    let text = fs::read_to_string("/proc/meminfo").context("could not read /proc/meminfo")?;
    let host = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .map(|kb| kb / 1024)
        .context("MemTotal is missing")?;
    let mut detected = host;
    for path in cgroup_memory_limit_paths() {
        if let Ok(value) = fs::read_to_string(path) {
            if let Ok(bytes) = value.trim().parse::<u64>() {
                let mb = bytes / 1024 / 1024;
                if mb > 0 && mb < detected {
                    detected = mb;
                }
            }
        }
    }
    if detected == 0 {
        bail!("detected zero memory")
    }
    Ok(detected)
}

fn cgroup_memory_limit_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/sys/fs/cgroup/memory.max"),
        PathBuf::from("/sys/fs/cgroup/memory/memory.limit_in_bytes"),
    ];
    if let Ok(cgroups) = fs::read_to_string("/proc/self/cgroup") {
        for line in cgroups.lines() {
            let mut fields = line.splitn(3, ':');
            let _id = fields.next();
            let controllers = fields.next().unwrap_or_default();
            let relative = fields.next().unwrap_or_default().trim_start_matches('/');
            if controllers.is_empty() {
                add_ancestor_limits(
                    &mut paths,
                    PathBuf::from("/sys/fs/cgroup").join(relative),
                    "memory.max",
                );
            } else if controllers
                .split(',')
                .any(|controller| controller == "memory")
            {
                add_ancestor_limits(
                    &mut paths,
                    PathBuf::from("/sys/fs/cgroup/memory").join(relative),
                    "memory.limit_in_bytes",
                );
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn add_ancestor_limits(paths: &mut Vec<PathBuf>, mut directory: PathBuf, file: &str) {
    let root = PathBuf::from("/sys/fs/cgroup");
    loop {
        paths.push(directory.join(file));
        if directory == root || !directory.pop() || !directory.starts_with(&root) {
            break;
        }
    }
}
