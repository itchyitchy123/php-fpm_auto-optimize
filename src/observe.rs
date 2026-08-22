use crate::model::{Evidence, Pool};
use std::{
    collections::{BTreeMap, HashMap},
    fs, thread,
    time::Duration,
};

pub fn observe(pools: &[Pool], samples: u32, interval: Duration) -> BTreeMap<String, Evidence> {
    let mut names: HashMap<&str, Vec<String>> = HashMap::new();
    for p in pools {
        names.entry(&p.id.name).or_default().push(format!(
            "{}:{}",
            p.id.directory.display(),
            p.id.name
        ));
    }
    let mut result = BTreeMap::new();
    let mut memory: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for sample in 0..samples.max(1) {
        let mut active: BTreeMap<String, u32> = BTreeMap::new();
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
                    continue;
                }
                let Ok(raw) = fs::read(entry.path().join("cmdline")) else {
                    continue;
                };
                let command = String::from_utf8_lossy(&raw).replace('\0', " ");
                let Some(pool_name) = command
                    .split("php-fpm: pool ")
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                else {
                    continue;
                };
                let Some(keys) = names.get(pool_name).filter(|v| v.len() == 1) else {
                    continue;
                };
                let key = &keys[0];
                *active.entry(key.clone()).or_default() += 1;
                if let Ok(status) = fs::read_to_string(entry.path().join("status")) {
                    if let Some(kb) = status.lines().find_map(|l| {
                        l.strip_prefix("VmRSS:")?
                            .split_whitespace()
                            .next()?
                            .parse::<u32>()
                            .ok()
                    }) {
                        memory
                            .entry(key.clone())
                            .or_default()
                            .push(kb.div_ceil(1024));
                    }
                }
            }
        }
        for (key, count) in active {
            let e = result.entry(key).or_insert_with(Evidence::default);
            e.peak_workers = e.peak_workers.max(Some(count));
        }
        for e in result.values_mut() {
            e.samples += 1;
        }
        if sample + 1 < samples {
            thread::sleep(interval);
        }
    }
    for (key, mut values) in memory {
        values.sort_unstable();
        if !values.is_empty() {
            let index = (values.len() * 75).div_ceil(100).saturating_sub(1);
            result.entry(key).or_default().worker_memory_mb = values.get(index).copied();
        }
    }
    result
}
