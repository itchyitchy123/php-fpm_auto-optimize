use crate::{config::PolicyFile, model::*};
use anyhow::{Result, bail};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn build_plan(
    pools: &[Pool],
    evidence: &BTreeMap<String, Evidence>,
    policy: &PolicyFile,
    host_memory_mb: u64,
) -> Result<Plan> {
    policy.validate()?;
    if host_memory_mb <= policy.global.reserve_memory_mb {
        bail!("reserved memory leaves no capacity for PHP-FPM");
    }
    let budget = (host_memory_mb - policy.global.reserve_memory_mb)
        * u64::from(policy.global.memory_utilization_percent)
        / 100;
    let mut decisions = Vec::with_capacity(pools.len());
    for pool in pools {
        let qualified = format!("{}:{}", pool.id.directory.display(), pool.id.name);
        let local = policy.for_pool(&pool.id.name, &qualified);
        let selected = local.selected.unwrap_or(true);
        let min = local
            .min_children
            .unwrap_or(policy.global.default_min_children);
        let max = local
            .max_children
            .unwrap_or(policy.global.default_max_children);
        if min > max {
            bail!("pool {qualified} has inverted child bounds");
        }
        let ev = evidence
            .get(&qualified)
            .or_else(|| evidence.get(&pool.id.name))
            .cloned()
            .unwrap_or_default();
        let worker_mb = ev
            .worker_memory_mb
            .unwrap_or(policy.global.default_worker_memory_mb)
            .max(1);
        let confidence = confidence(&ev, policy.global.minimum_evidence_samples);
        let current = pool.settings.max_children.unwrap_or(min).clamp(min, max);
        let mut reasons = Vec::new();
        let target = if !selected {
            reasons.push("not selected; current settings retained".into());
            current
        } else if confidence != Confidence::Low && ev.peak_workers.is_some() {
            let peak = ev.peak_workers.unwrap_or(current);
            let headroom = u32::try_from(div_ceil(
                u64::from(peak) * (100 + u64::from(policy.global.headroom_percent)),
                100,
            ))
            .unwrap_or(u32::MAX);
            reasons.push(format!(
                "observed peak {peak} with {}% headroom",
                policy.global.headroom_percent
            ));
            headroom
        } else if ev.saturation_events > 0 {
            reasons.push(format!(
                "{} saturation event(s); preserving capacity with headroom",
                ev.saturation_events
            ));
            u32::try_from(div_ceil(u64::from(current) * 115, 100)).unwrap_or(u32::MAX)
        } else {
            reasons.push("insufficient representative evidence; current capacity retained".into());
            current
        }
        .clamp(min, max);
        let target = if let Some(explicit) = local.target_children {
            reasons.push("explicit user-selected target".into());
            explicit.clamp(min, max)
        } else {
            target
        };
        if target == min {
            reasons.push("minimum bound enforced".into());
        }
        if target == max {
            reasons.push("maximum bound enforced".into());
        }
        let mut proposed = pool.settings.clone();
        proposed.max_children = Some(target);
        if let Some(v) = local.max_requests {
            proposed.max_requests = Some(v);
        }
        if let Some(v) = local.process_idle_timeout_seconds {
            proposed.process_idle_timeout_seconds = Some(v);
        }
        if let Some(v) = local.request_terminate_timeout_seconds {
            proposed.request_terminate_timeout_seconds = Some(v);
        }
        normalize_dynamic(&mut proposed);
        decisions.push(PoolDecision {
            id: pool.id.clone(),
            selected,
            current: pool.settings.clone(),
            proposed,
            minimum_children: min,
            maximum_children: max,
            worker_memory_mb: worker_mb,
            evidence: ev,
            confidence,
            reasons,
        });
    }

    let mut warnings = Vec::new();
    let floor_memory: u64 = decisions
        .iter()
        .map(|d| {
            u64::from(if d.selected {
                d.minimum_children
            } else {
                d.proposed.max_children.unwrap_or(0)
            }) * u64::from(d.worker_memory_mb)
        })
        .sum();
    let feasible = floor_memory <= budget;
    if feasible {
        constrain_to_budget(&mut decisions, budget);
    } else {
        warnings.push(format!("minimum and fixed allocations require {floor_memory} MB but the FPM budget is {budget} MB"));
    }
    let allocated = memory_for(&decisions);
    if decisions.iter().any(|d| d.confidence == Confidence::Low) {
        warnings.push("one or more pools lack representative observations; their current capacity was preserved".into());
    }
    Ok(Plan {
        schema_version: 1,
        generated_at_unix: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        host_memory_mb,
        available_fpm_memory_mb: budget,
        allocated_memory_mb: allocated,
        feasible,
        warnings,
        pools: decisions,
    })
}

fn confidence(e: &Evidence, minimum: u32) -> Confidence {
    if e.samples >= minimum.saturating_mul(4)
        && e.worker_memory_mb.is_some()
        && e.peak_workers.is_some()
    {
        Confidence::High
    } else if e.samples >= minimum && (e.worker_memory_mb.is_some() || e.peak_workers.is_some()) {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn constrain_to_budget(decisions: &mut [PoolDecision], budget: u64) {
    while memory_for(decisions) > budget {
        let candidate = decisions
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                d.selected && d.proposed.max_children.unwrap_or(0) > d.minimum_children
            })
            .min_by_key(|(_, d)| (priority(d), d.proposed.max_children.unwrap_or(0)))
            .map(|(i, _)| i);
        let Some(i) = candidate else { break };
        let value = decisions[i].proposed.max_children.unwrap_or(1) - 1;
        decisions[i].proposed.max_children = Some(value);
        if !decisions[i]
            .reasons
            .iter()
            .any(|r| r == "reduced to satisfy the host memory constraint")
        {
            decisions[i]
                .reasons
                .push("reduced to satisfy the host memory constraint".into());
        }
        normalize_dynamic(&mut decisions[i].proposed);
    }
}

fn priority(d: &PoolDecision) -> (u8, u32, u32) {
    let confidence = match d.confidence {
        Confidence::Low => 0,
        Confidence::Medium => 1,
        Confidence::High => 2,
    };
    (
        confidence,
        d.evidence.saturation_events,
        d.evidence.peak_workers.unwrap_or(0),
    )
}

fn normalize_dynamic(s: &mut FpmSettings) {
    if s.pm != ProcessManager::Dynamic {
        return;
    }
    let cap = s.max_children.unwrap_or(u32::MAX);
    s.start_servers = s.start_servers.map(|v| v.min(cap));
    s.min_spare_servers = s.min_spare_servers.map(|v| v.min(cap));
    s.max_spare_servers = s.max_spare_servers.map(|v| v.min(cap));
}

fn memory_for(decisions: &[PoolDecision]) -> u64 {
    decisions
        .iter()
        .map(|d| u64::from(d.proposed.max_children.unwrap_or(0)) * u64::from(d.worker_memory_mb))
        .sum()
}
fn div_ceil(a: u64, b: u64) -> u64 {
    a.div_ceil(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn pool(name: &str, current: u32) -> Pool {
        Pool {
            id: PoolId {
                directory: PathBuf::from("/p"),
                name: name.into(),
            },
            source_files: vec![],
            settings: FpmSettings {
                max_children: Some(current),
                ..Default::default()
            },
        }
    }
    #[test]
    fn no_evidence_does_not_mean_quiet() {
        let plan = build_plan(
            &[pool("www", 20)],
            &BTreeMap::new(),
            &PolicyFile::default(),
            4096,
        )
        .unwrap();
        assert_eq!(plan.pools[0].proposed.max_children, Some(20));
    }
    #[test]
    fn heterogeneous_costs_fit_budget_and_bounds() {
        let mut policy = PolicyFile::default();
        policy.global.reserve_memory_mb = 0;
        policy.global.memory_utilization_percent = 100;
        policy.global.default_min_children = 2;
        policy.global.default_max_children = 100;
        let mut ev = BTreeMap::new();
        ev.insert(
            "a".into(),
            Evidence {
                peak_workers: Some(20),
                worker_memory_mb: Some(100),
                samples: 50,
                ..Default::default()
            },
        );
        ev.insert(
            "b".into(),
            Evidence {
                peak_workers: Some(20),
                worker_memory_mb: Some(25),
                samples: 50,
                ..Default::default()
            },
        );
        let plan = build_plan(&[pool("a", 20), pool("b", 20)], &ev, &policy, 1000).unwrap();
        assert!(
            plan.allocated_memory_mb <= 1000
                && plan
                    .pools
                    .iter()
                    .all(|p| p.proposed.max_children.unwrap() >= 2)
        );
    }
    #[test]
    fn reports_infeasible_minima() {
        let mut policy = PolicyFile::default();
        policy.global.reserve_memory_mb = 0;
        policy.global.memory_utilization_percent = 100;
        policy.global.default_min_children = 10;
        let plan = build_plan(
            &[pool("a", 20), pool("b", 20)],
            &BTreeMap::new(),
            &policy,
            100,
        )
        .unwrap();
        assert!(!plan.feasible);
    }
}
