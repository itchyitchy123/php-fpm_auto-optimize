use crate::model::{Evidence, Pool};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn observe(pools: &[Pool], samples: u32, interval: Duration) -> BTreeMap<String, Evidence> {
    observe_with_status(pools, samples, interval, &BTreeMap::new())
}

/// Collect memory from procfs and demand from configured PHP-FPM JSON status URLs.
pub fn observe_with_status(
    pools: &[Pool],
    samples: u32,
    interval: Duration,
    status_urls: &BTreeMap<String, String>,
) -> BTreeMap<String, Evidence> {
    let mut names: HashMap<&str, Vec<String>> = HashMap::new();
    for p in pools {
        names.entry(&p.id.name).or_default().push(format!(
            "{}:{}",
            p.id.directory.display(),
            p.id.name
        ));
    }
    let mut result = BTreeMap::new();
    for keys in names.values() {
        for key in keys {
            result.insert(key.clone(), Evidence::default());
        }
    }
    let mut memory: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut methods: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    let mut saturation_counters: BTreeMap<String, u32> = BTreeMap::new();
    let started = SystemTime::now();
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal_enabled =
        signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted)).is_ok();
    for sample in 0..samples.max(1) {
        if interrupted.load(Ordering::Relaxed) {
            break;
        }
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
                let pss = read_memory_kb(&entry.path().join("smaps_rollup"), "Pss:");
                let rss = read_memory_kb(&entry.path().join("status"), "VmRSS:");
                if let Some(kb) = pss.or(rss) {
                    let evidence = result.entry(key.clone()).or_default();
                    evidence.memory_samples += 1;
                    let seen = methods.entry(key.clone()).or_default();
                    if pss.is_some() {
                        seen.0 = true;
                    } else {
                        seen.1 = true;
                    }
                    memory
                        .entry(key.clone())
                        .or_default()
                        .push(kb.div_ceil(1024));
                }
            }
        }
        let mut statuses = Vec::with_capacity(status_urls.len());
        for chunk in status_urls.iter().collect::<Vec<_>>().chunks(16) {
            thread::scope(|scope| {
                let mut handles = Vec::with_capacity(chunk.len());
                for (key, url) in chunk {
                    handles.push(((*key).clone(), scope.spawn(|| fetch_status(url))));
                }
                for (key, handle) in handles {
                    statuses.push((
                        key,
                        handle
                            .join()
                            .unwrap_or_else(|_| Err("status worker panicked".into())),
                    ));
                }
            });
        }
        for (key, status) in statuses {
            let e = result.entry(key.clone()).or_default();
            e.status_attempts += 1;
            match status {
                Ok(status) => {
                    e.peak_workers = e.peak_workers.max(Some(status.active));
                    e.listen_queue_peak = e.listen_queue_peak.max(Some(status.queue));
                    if let Some(previous) =
                        saturation_counters.insert(key.clone(), status.max_children_reached)
                    {
                        e.saturation_events = e.saturation_events.saturating_add(
                            status
                                .max_children_reached
                                .checked_sub(previous)
                                .unwrap_or(status.max_children_reached),
                        );
                    }
                    e.status_samples += 1;
                }
                Err(error) => {
                    let warning = format!("status endpoint unavailable: {error}");
                    if !e.warnings.contains(&warning) {
                        e.warnings.push(warning);
                    }
                }
            }
        }
        for e in result.values_mut() {
            e.samples += 1;
        }
        if sample + 1 < samples {
            sleep_interruptible(interval, &interrupted);
        }
    }
    for (key, mut values) in memory {
        values.sort_unstable();
        if !values.is_empty() {
            let e = result.entry(key).or_default();
            e.memory_p50_mb = percentile(&values, 50);
            e.worker_memory_mb = percentile(&values, 75);
            e.memory_p95_mb = percentile(&values, 95);
            e.memory_max_mb = values.last().copied();
        }
    }
    for (key, (pss, rss)) in methods {
        result.entry(key).or_default().memory_measurement = Some(
            match (pss, rss) {
                (true, true) => "mixed",
                (true, false) => "pss",
                (false, true) => "rss",
                (false, false) => continue,
            }
            .into(),
        );
    }
    let elapsed = started.elapsed().unwrap_or_default().as_secs();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for e in result.values_mut() {
        e.observed_at_unix = Some(now);
        e.observation_seconds = Some(elapsed);
        e.complete = !interrupted.load(Ordering::Relaxed);
        if !e.complete {
            e.warnings
                .push("observation interrupted; partial evidence saved".into());
        }
        if !signal_enabled {
            e.warnings.push(
                "could not install SIGINT handler; interruption may not save evidence".into(),
            );
        }
        if e.status_samples == 0 {
            e.warnings
                .push("no PHP-FPM status samples; active demand is unknown".into());
        }
    }
    result
}

fn sleep_interruptible(duration: Duration, interrupted: &AtomicBool) {
    let deadline = std::time::Instant::now() + duration;
    while !interrupted.load(Ordering::Relaxed) {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(100)));
    }
}

fn percentile(values: &[u32], percent: usize) -> Option<u32> {
    values
        .get((values.len() * percent).div_ceil(100).saturating_sub(1))
        .copied()
}

fn read_memory_kb(path: &std::path::Path, field: &str) -> Option<u32> {
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        line.strip_prefix(field)?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

struct Status {
    active: u32,
    queue: u32,
    max_children_reached: u32,
}

fn fetch_status(url: &str) -> Result<Status, String> {
    let parsed = parse_status_url(url)?;
    let addresses = (parsed.host, parsed.port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?;
    let connect_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut last_error = None;
    let mut stream = None;
    for address in addresses.take(4) {
        let remaining = connect_deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(&address, remaining) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let mut stream = stream.ok_or_else(|| {
        last_error.map_or_else(
            || "status host did not resolve".into(),
            |error| error.to_string(),
        )
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    write!(
        stream,
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        parsed.path, parsed.authority
    )
    .map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream
        .take(1_048_577)
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;
    if response.len() > 1_048_576 {
        return Err("status response exceeds 1 MiB".into());
    }
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or("invalid HTTP response")?;
    if head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .is_none_or(|status| status != "200")
    {
        return Err("HTTP status was not 200".into());
    }
    parse_status(body)
}

pub fn validate_status_url(url: &str) -> Result<(), String> {
    parse_status_url(url).map(|_| ())
}

struct ParsedStatusUrl<'a> {
    authority: &'a str,
    host: &'a str,
    port: u16,
    path: Cow<'a, str>,
}

fn parse_status_url(url: &str) -> Result<ParsedStatusUrl<'_>, String> {
    if url
        .chars()
        .any(|character| character.is_control() || character == ' ')
    {
        return Err("status URL contains whitespace or control characters".into());
    }
    let rest = url
        .strip_prefix("http://")
        .ok_or("only http:// status URLs are supported")?;
    if rest.contains('#') {
        return Err("status URL must not contain a fragment".into());
    }
    let boundary = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..boundary];
    let suffix = &rest[boundary..];
    let path = if suffix.is_empty() {
        Cow::Borrowed("/")
    } else if suffix.starts_with('?') {
        Cow::Owned(format!("/{suffix}"))
    } else {
        Cow::Borrowed(suffix)
    };
    if authority.contains('@') {
        return Err("status URL must not contain user information".into());
    }
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, remainder) = bracketed
            .split_once(']')
            .ok_or("invalid bracketed IPv6 status URL host")?;
        let port = if remainder.is_empty() {
            80
        } else {
            remainder
                .strip_prefix(':')
                .ok_or("invalid bracketed IPv6 status URL authority")?
                .parse::<u16>()
                .map_err(|_| "invalid status URL port")?
        };
        (host, port)
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) => {
                if host.contains(':') {
                    return Err("IPv6 status URL hosts must be enclosed in brackets".into());
                }
                (
                    host,
                    port.parse::<u16>().map_err(|_| "invalid status URL port")?,
                )
            }
            None => (authority, 80),
        }
    };
    if host.is_empty() {
        return Err("status URL host is empty".into());
    }
    Ok(ParsedStatusUrl {
        authority,
        host,
        port,
        path,
    })
}

fn parse_status(body: &str) -> Result<Status, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))?;
    let number = |name: &str| -> Result<u32, String> {
        value
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| format!("missing or invalid {name:?}"))
    };
    Ok(Status {
        active: number("active processes")?,
        queue: number("listen queue")?,
        max_children_reached: number("max children reached")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_standard_json_status_fields() {
        let status =
            parse_status(r#"{"active processes":7,"listen queue":2,"max children reached":3}"#)
                .unwrap();
        assert_eq!(status.active, 7);
        assert_eq!(status.queue, 2);
        assert_eq!(status.max_children_reached, 3);
    }

    #[test]
    fn rejects_missing_or_invalid_status_fields() {
        assert!(parse_status(r#"{"active processes":0}"#).is_err());
        assert!(
            parse_status(r#"{"active processes":"0","listen queue":0,"max children reached":0}"#)
                .is_err()
        );
    }

    #[test]
    fn validates_status_urls_without_network_access() {
        assert!(validate_status_url("http://127.0.0.1/status?json").is_ok());
        assert!(validate_status_url("http://localhost?json").is_ok());
        assert!(validate_status_url("http://[::1]:8080/status?json").is_ok());
        assert!(validate_status_url("https://127.0.0.1/status").is_err());
        assert!(validate_status_url("http://user@127.0.0.1/status").is_err());
        assert!(validate_status_url("http://127.0.0.1/status#fragment").is_err());
    }
}
