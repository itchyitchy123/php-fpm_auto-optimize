use crate::{Evidence, Plan};
use anyhow::{Context, Result, bail};
use sha2::Digest;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
    process::Command,
};

const MAX_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;

pub fn load_plan(path: &Path) -> Result<Plan> {
    let plan: Plan = serde_json::from_slice(&read_limited(path)?)
        .with_context(|| format!("invalid plan artifact {}", path.display()))?;
    validate_plan(&plan)?;
    Ok(plan)
}

pub fn validate_evidence_map(evidence: &BTreeMap<String, Evidence>) -> Result<()> {
    for (key, value) in evidence {
        if key.is_empty() || key.chars().any(char::is_control) {
            bail!("invalid evidence key {key:?}");
        }
        validate_evidence(key, value)?;
    }
    Ok(())
}

fn validate_evidence(name: &str, evidence: &Evidence) -> Result<()> {
    if evidence.status_samples > evidence.status_attempts {
        bail!("evidence for {name:?} has more successful than attempted status samples");
    }
    if evidence.worker_memory_mb == Some(0)
        || evidence.memory_p50_mb == Some(0)
        || evidence.memory_p95_mb == Some(0)
        || evidence.memory_max_mb == Some(0)
    {
        bail!("evidence for {name:?} contains a zero memory measurement");
    }
    if !matches!(
        evidence.memory_measurement.as_deref(),
        None | Some("pss" | "rss" | "mixed")
    ) {
        bail!("evidence for {name:?} has an unknown memory measurement method");
    }
    if evidence
        .warnings
        .iter()
        .any(|warning| warning.chars().any(char::is_control))
    {
        bail!("evidence for {name:?} contains unsafe diagnostic text");
    }
    let percentiles = [
        evidence.memory_p50_mb,
        evidence.worker_memory_mb,
        evidence.memory_p95_mb,
        evidence.memory_max_mb,
    ];
    if percentiles
        .into_iter()
        .flatten()
        .try_fold(0, |previous, value| (value >= previous).then_some(value))
        .is_none()
    {
        bail!("evidence for {name:?} has inconsistent memory percentiles");
    }
    Ok(())
}

pub fn validate_plan(plan: &Plan) -> Result<()> {
    if plan.schema_version != 1 {
        bail!("unsupported plan schema version {}", plan.schema_version);
    }
    if !plan.feasible {
        bail!("plan is marked infeasible");
    }
    if plan.host_memory_mb == 0 || plan.available_fpm_memory_mb == 0 {
        bail!("host memory and plan budget must be positive");
    }
    if plan.allocated_memory_mb > plan.available_fpm_memory_mb {
        bail!("allocated memory exceeds the plan budget");
    }
    if plan.available_fpm_memory_mb > plan.host_memory_mb {
        bail!("plan budget exceeds host memory");
    }
    let mut calculated = 0_u64;
    if plan
        .warnings
        .iter()
        .any(|value| value.chars().any(char::is_control))
    {
        bail!("plan warnings contain control characters");
    }
    let mut ids = BTreeSet::new();
    for pool in &plan.pools {
        if pool.id.name.is_empty()
            || pool.id.name.trim() != pool.id.name
            || pool.id.name.chars().any(char::is_control)
            || pool.id.name.contains(['[', ']'])
            || pool.minimum_children == 0
            || pool.minimum_children > pool.maximum_children
            || pool.worker_memory_mb == 0
        {
            bail!("invalid pool entry for {:?}", pool.id.name);
        }
        let directory = pool
            .id
            .directory
            .to_str()
            .context("pool directory is not UTF-8")?;
        if directory.is_empty() || directory.chars().any(char::is_control) {
            bail!("pool {} has an unsafe source directory", pool.id.name);
        }
        let current = pool
            .current
            .max_children
            .context("current max_children is missing")?;
        let proposed = pool
            .proposed
            .max_children
            .context("proposed max_children is missing")?;
        if current == 0 || proposed == 0 {
            bail!("pool {} has a zero max_children", pool.id.name);
        }
        if !pool.selected && pool.current != pool.proposed {
            bail!("unselected pool {} contains proposed changes", pool.id.name);
        }
        let id = (pool.id.directory.clone(), pool.id.name.clone());
        if !ids.insert(id) {
            bail!("duplicate pool entry for {}", pool.id.name);
        }
        if pool.selected && !(pool.minimum_children..=pool.maximum_children).contains(&proposed) {
            bail!("pool {} proposal is outside its bounds", pool.id.name);
        }
        validate_dynamic(&pool.id.name, &pool.proposed)?;
        calculated = calculated
            .checked_add(u64::from(proposed) * u64::from(pool.worker_memory_mb))
            .context("plan allocation overflow")?;
        if pool
            .reasons
            .iter()
            .any(|value| value.chars().any(char::is_control))
        {
            bail!("pool {} contains unsafe diagnostic text", pool.id.name);
        }
        validate_evidence(&pool.id.name, &pool.evidence)?;
    }
    if calculated != plan.allocated_memory_mb {
        bail!(
            "allocated memory is inconsistent: artifact claims {} MB, calculated {calculated} MB",
            plan.allocated_memory_mb
        );
    }
    Ok(())
}

fn read_limited(path: &Path) -> Result<Vec<u8>> {
    let file =
        fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    if file.metadata()?.len() > MAX_ARTIFACT_BYTES {
        bail!("artifact exceeds 16 MiB");
    }
    let mut bytes = Vec::new();
    file.take(MAX_ARTIFACT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        bail!("artifact exceeds 16 MiB");
    }
    Ok(bytes)
}

fn validate_dynamic(name: &str, settings: &crate::FpmSettings) -> Result<()> {
    if settings.pm != crate::ProcessManager::Dynamic {
        return Ok(());
    }
    let cap = settings
        .max_children
        .context("dynamic pool max_children is missing")?;
    if settings.start_servers.is_some_and(|v| v > cap)
        || settings.min_spare_servers.is_some_and(|v| v > cap)
        || settings.max_spare_servers.is_some_and(|v| v > cap)
        || settings
            .min_spare_servers
            .zip(settings.max_spare_servers)
            .is_some_and(|(a, b)| a > b)
    {
        bail!("dynamic settings are inconsistent for pool {name}");
    }
    Ok(())
}

pub fn verify_php_fpm(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("-tt")
        .output()
        .with_context(|| format!("could not execute {}", binary.display()))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!(
            "{} -tt rejected the installed configuration:\n{text}",
            binary.display()
        );
    }
    Ok(text)
}

/// Render and validate each affected source directory with the requested PHP-FPM binary.
pub fn verify_staged_plan(plan: &Plan, binary: &Path) -> Result<String> {
    validate_plan(plan)?;
    let temporary = tempfile::tempdir().context("could not create validation workspace")?;
    let staged_root = temporary.path().join("rendered");
    let staged = crate::render::render_overrides(plan, &staged_root)?;
    let mut output = String::new();
    for (index, staged_file) in staged.iter().enumerate() {
        let source = plan
            .pools
            .iter()
            .find(|pool| {
                pool.selected
                    && pool.current != pool.proposed
                    && staged_file.parent().is_some_and(|parent| {
                        let digest = format!(
                            "{:x}",
                            sha2::Sha256::digest(pool.id.directory.as_os_str().as_encoded_bytes())
                        );
                        parent
                            .file_name()
                            .is_some_and(|name| name == digest.as_str())
                    })
            })
            .context("could not map staged override to its source directory")?;
        if !source.id.directory.is_dir() {
            bail!(
                "cannot validate staged override: source directory {} is unavailable",
                source.id.directory.display()
            );
        }
        let source_directory = fs::canonicalize(&source.id.directory)?;
        let staged_file = fs::canonicalize(staged_file)?;
        let master = temporary.path().join(format!("php-fpm-{index}.conf"));
        let config = format!(
            "[global]\ndaemonize = no\ninclude = {}/*.conf\ninclude = {}\n",
            source_directory.display(),
            staged_file.display()
        );
        fs::write(&master, config)?;
        let result = Command::new(binary)
            .args(["-tt", "-y"])
            .arg(&master)
            .output()
            .with_context(|| format!("could not execute {}", binary.display()))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        if !result.status.success() {
            bail!(
                "{} rejected staged overrides for {}:\n{text}",
                binary.display(),
                source_directory.display()
            );
        }
        output.push_str(&format!("validated {}\n{text}", source_directory.display()));
    }
    if staged.is_empty() {
        output.push_str("plan contains no configuration changes\n");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Confidence, Evidence, FpmSettings, PoolDecision, PoolId};
    use std::path::PathBuf;

    fn plan() -> Plan {
        Plan {
            schema_version: 1,
            generated_at_unix: 0,
            host_memory_mb: 100,
            available_fpm_memory_mb: 100,
            allocated_memory_mb: 10,
            feasible: true,
            warnings: vec![],
            pools: vec![PoolDecision {
                id: PoolId {
                    directory: PathBuf::from("/etc/php"),
                    name: "www".into(),
                },
                selected: true,
                current: FpmSettings {
                    max_children: Some(1),
                    ..Default::default()
                },
                proposed: FpmSettings {
                    max_children: Some(2),
                    ..Default::default()
                },
                minimum_children: 1,
                maximum_children: 3,
                worker_memory_mb: 5,
                evidence: Evidence::default(),
                confidence: Confidence::Low,
                reasons: vec![],
            }],
        }
    }

    #[test]
    fn rejects_unsafe_pool_names() {
        let mut value = plan();
        value.pools[0].id.name = "www]\npm.max_children=999".into();
        assert!(validate_plan(&value).is_err());
    }

    #[test]
    fn rejects_budget_violation() {
        let mut value = plan();
        value.allocated_memory_mb = 101;
        assert!(validate_plan(&value).is_err());
    }

    #[test]
    fn rejects_dishonest_allocation_total() {
        let mut value = plan();
        value.allocated_memory_mb = 1;
        assert!(
            validate_plan(&value)
                .unwrap_err()
                .to_string()
                .contains("inconsistent")
        );
    }

    #[test]
    fn rejects_duplicate_pool_ids() {
        let mut value = plan();
        value.allocated_memory_mb = 20;
        value.pools.push(value.pools[0].clone());
        assert!(
            validate_plan(&value)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn rejects_changes_to_unselected_pools() {
        let mut value = plan();
        value.pools[0].selected = false;
        assert!(validate_plan(&value).is_err());
    }

    #[test]
    fn rejects_malformed_evidence() {
        let mut evidence = BTreeMap::new();
        evidence.insert(
            "www".into(),
            Evidence {
                worker_memory_mb: Some(20),
                memory_p95_mb: Some(10),
                ..Default::default()
            },
        );
        assert!(validate_evidence_map(&evidence).is_err());

        evidence.get_mut("www").unwrap().memory_p95_mb = Some(30);
        evidence.get_mut("www").unwrap().memory_measurement = Some("guess".into());
        assert!(validate_evidence_map(&evidence).is_err());
    }

    #[test]
    fn staged_validation_uses_explicit_binary_and_source_configuration() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("pool.d");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("www.conf"),
            "[www]\npm=ondemand\npm.max_children=1\n",
        )
        .unwrap();
        let binary = temp.path().join("fake-php-fpm");
        fs::write(
            &binary,
            "#!/bin/sh\ntest \"$1\" = -tt && test \"$2\" = -y\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).unwrap();
        let mut value = plan();
        value.pools[0].id.directory = source;
        let output = verify_staged_plan(&value, &binary).unwrap();
        assert!(output.contains("validated"));
    }
}
