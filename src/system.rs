use anyhow::{Context, Result, bail};
use std::fs;

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
    for path in [
        "/sys/fs/cgroup/memory.max",
        "/sys/fs/cgroup/memory/memory.limit_in_bytes",
    ] {
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
