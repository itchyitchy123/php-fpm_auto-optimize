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
    crate::artifact::validate_evidence_map(evidence)?;
    if host_memory_mb <= policy.global.reserve_memory_mb {
        bail!("reserved memory leaves no capacity for PHP-FPM");
    }
    let budget = (host_memory_mb - policy.global.reserve_memory_mb)
        .checked_mul(u64::from(policy.global.memory_utilization_percent))
        .ok_or_else(|| anyhow::anyhow!("memory budget overflow"))?
        / 100;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
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
        let confidence = confidence(&ev, &policy.global, now);
        let fresh = evidence_is_fresh(&ev, &policy.global, now);
        let configured = pool.settings.max_children.unwrap_or(min);
        let current = if selected {
            configured.clamp(min, max)
        } else {
            configured
        };
        let mut reasons = Vec::new();
        if selected && current != configured {
            reasons.push(format!(
                "configured capacity {configured} adjusted to explicit policy bounds {min}..={max}"
            ));
        }
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
        } else if fresh && ev.saturation_events > 0 {
            reasons.push(format!(
                "{} saturation event(s); preserving capacity with headroom",
                ev.saturation_events
            ));
            u32::try_from(div_ceil(u64::from(current) * 115, 100)).unwrap_or(u32::MAX)
        } else {
            reasons.push(if current == configured {
                "insufficient representative evidence; current capacity retained".into()
            } else {
                "insufficient representative evidence; explicit policy bounds applied".into()
            });
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
        if selected {
            if let Some(v) = local.max_requests {
                proposed.max_requests = Some(v);
            }
            if let Some(v) = local.process_idle_timeout_seconds {
                if proposed.pm == ProcessManager::Ondemand {
                    proposed.process_idle_timeout_seconds = Some(v);
                } else {
                    reasons.push(
                        "pm.process_idle_timeout ignored: it is valid only for ondemand pools"
                            .into(),
                    );
                }
            }
            if let Some(v) = local.request_terminate_timeout_seconds {
                proposed.request_terminate_timeout_seconds = Some(v);
            }
            normalize_dynamic(&mut proposed);
        }
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
    let floor_memory = decisions.iter().try_fold(0_u64, |total, d| {
        let children = if d.selected {
            d.minimum_children
        } else {
            d.proposed.max_children.unwrap_or(0)
        };
        total
            .checked_add(u64::from(children) * u64::from(d.worker_memory_mb))
            .ok_or_else(|| anyhow::anyhow!("minimum allocation overflow"))
    })?;
    let mut feasible = floor_memory <= budget;
    if feasible {
        constrain_to_budget(&mut decisions, budget)?;
    } else {
        warnings.push(format!("minimum and fixed allocations require {floor_memory} MB but the FPM budget is {budget} MB"));
    }
    let allocated = memory_for(&decisions)?;
    if allocated > budget {
        feasible = false;
        warnings.push(format!(
            "preserving uncertain capacity requires {allocated} MB but the FPM budget is {budget} MB; collect status evidence or explicitly review targets"
        ));
    }
    if decisions.iter().any(|d| d.confidence == Confidence::Low) {
        warnings.push("one or more pools lack fresh, representative observations; review every bounds-driven or explicit change".into());
    }
    for decision in &decisions {
        for warning in &decision.evidence.warnings {
            warnings.push(format!("pool {} evidence: {warning}", decision.id.name));
        }
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

fn confidence(e: &Evidence, policy: &GlobalPolicy, now: u64) -> Confidence {
    if !e.complete
        || !evidence_is_fresh(e, policy, now)
        || e.status_attempts == 0
        || e.observation_seconds.unwrap_or(0) < policy.minimum_observation_seconds
    {
        return Confidence::Low;
    }
    let success = u64::from(e.status_samples) * 100 / u64::from(e.status_attempts);
    if success < u64::from(policy.minimum_status_success_percent) {
        return Confidence::Low;
    }
    if e.status_samples >= policy.minimum_evidence_samples.saturating_mul(4)
        && e.worker_memory_mb.is_some()
        && e.memory_samples > 0
        && e.peak_workers.is_some()
    {
        Confidence::High
    } else if e.status_samples >= policy.minimum_evidence_samples && e.peak_workers.is_some() {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn evidence_is_fresh(e: &Evidence, policy: &GlobalPolicy, now: u64) -> bool {
    e.observed_at_unix.is_some_and(|observed| {
        observed <= now.saturating_add(300)
            && now.saturating_sub(observed) <= policy.maximum_evidence_age_seconds
    })
}

fn constrain_to_budget(decisions: &mut [PoolDecision], budget: u64) -> Result<()> {
    loop {
        let allocated = memory_for(decisions)?;
        if allocated <= budget {
            break;
        }
        let candidate = decisions
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                d.selected
                    && d.confidence != Confidence::Low
                    && d.proposed.max_children.unwrap_or(0) > d.minimum_children
            })
            .min_by_key(|(_, d)| (priority(d), d.proposed.max_children.unwrap_or(0)))
            .map(|(i, _)| i);
        let Some(i) = candidate else { break };
        let current = decisions[i].proposed.max_children.unwrap_or(1);
        let removable = current - decisions[i].minimum_children;
        let needed = (allocated - budget).div_ceil(u64::from(decisions[i].worker_memory_mb));
        let reduction = u32::try_from(needed)
            .unwrap_or(u32::MAX)
            .min(removable)
            .max(1);
        let value = current - reduction;
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
    Ok(())
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

fn memory_for(decisions: &[PoolDecision]) -> Result<u64> {
    decisions.iter().try_fold(0_u64, |total, d| {
        total
            .checked_add(
                u64::from(d.proposed.max_children.unwrap_or(0)) * u64::from(d.worker_memory_mb),
            )
            .ok_or_else(|| anyhow::anyhow!("plan allocation overflow"))
    })
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
                status_samples: 50,
                status_attempts: 50,
                memory_samples: 50,
                observation_seconds: Some(300),
                observed_at_unix: Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                ),
                complete: true,
                ..Default::default()
            },
        );
        ev.insert(
            "b".into(),
            Evidence {
                peak_workers: Some(20),
                worker_memory_mb: Some(25),
                samples: 50,
                status_samples: 50,
                status_attempts: 50,
                memory_samples: 50,
                observation_seconds: Some(300),
                observed_at_unix: Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                ),
                complete: true,
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
    #[test]
    fn uncertain_capacity_is_not_silently_reduced() {
        let mut policy = PolicyFile::default();
        policy.global.reserve_memory_mb = 0;
        policy.global.memory_utilization_percent = 100;
        policy.global.default_min_children = 1;
        let plan = build_plan(&[pool("www", 20)], &BTreeMap::new(), &policy, 100).unwrap();
        assert_eq!(plan.pools[0].proposed.max_children, Some(20));
        assert!(!plan.feasible);
        assert!(plan.warnings.iter().any(|w| w.contains("uncertain")));
    }
    #[test]
    fn stale_evidence_cannot_reduce_capacity() {
        let mut policy = PolicyFile::default();
        policy.global.reserve_memory_mb = 0;
        let mut evidence = BTreeMap::new();
        evidence.insert(
            "www".into(),
            Evidence {
                peak_workers: Some(1),
                worker_memory_mb: Some(10),
                status_samples: 100,
                status_attempts: 100,
                memory_samples: 100,
                observation_seconds: Some(3600),
                observed_at_unix: Some(1),
                complete: true,
                ..Default::default()
            },
        );
        let plan = build_plan(&[pool("www", 20)], &evidence, &policy, 4096).unwrap();
        assert_eq!(plan.pools[0].confidence, Confidence::Low);
        assert_eq!(plan.pools[0].proposed.max_children, Some(20));
    }

    #[test]
    fn bounds_driven_low_confidence_change_is_explained() {
        let mut policy = PolicyFile::default();
        policy.global.default_max_children = 10;
        let plan = build_plan(&[pool("www", 20)], &BTreeMap::new(), &policy, 4096).unwrap();
        assert_eq!(plan.pools[0].proposed.max_children, Some(10));
        assert!(
            plan.pools[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("policy bounds"))
        );
    }

    #[test]
    fn unselected_pool_retains_every_setting() {
        let mut policy = PolicyFile::default();
        policy.pools.insert(
            "www".into(),
            PoolPolicy {
                selected: Some(false),
                max_requests: Some(500),
                process_idle_timeout_seconds: Some(30),
                ..Default::default()
            },
        );
        let plan = build_plan(&[pool("www", 20)], &BTreeMap::new(), &policy, 4096).unwrap();
        assert_eq!(plan.pools[0].current, plan.pools[0].proposed);
    }

    #[test]
    fn feasible_plans_always_respect_budget_and_bounds() {
        for worker_mb in [1, 7, 64, 511] {
            for peak in [1, 3, 25, 100] {
                for host in [64, 512, 4096] {
                    let mut policy = PolicyFile::default();
                    policy.global.reserve_memory_mb = 0;
                    policy.global.memory_utilization_percent = 100;
                    policy.global.default_min_children = 1;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let evidence = BTreeMap::from([(
                        "www".into(),
                        Evidence {
                            peak_workers: Some(peak),
                            worker_memory_mb: Some(worker_mb),
                            samples: 100,
                            status_samples: 100,
                            status_attempts: 100,
                            memory_samples: 100,
                            observation_seconds: Some(600),
                            observed_at_unix: Some(now),
                            complete: true,
                            ..Default::default()
                        },
                    )]);
                    let plan = build_plan(&[pool("www", 10)], &evidence, &policy, host).unwrap();
                    if plan.feasible {
                        assert!(plan.allocated_memory_mb <= plan.available_fpm_memory_mb);
                        let value = plan.pools[0].proposed.max_children.unwrap();
                        assert!(
                            (plan.pools[0].minimum_children..=plan.pools[0].maximum_children)
                                .contains(&value)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn idle_timeout_is_never_proposed_for_a_dynamic_pool() {
        let mut policy = PolicyFile::default();
        policy.pools.insert(
            "www".into(),
            PoolPolicy {
                process_idle_timeout_seconds: Some(15),
                ..Default::default()
            },
        );
        let mut dynamic = pool("www", 20);
        dynamic.settings.pm = ProcessManager::Dynamic;
        let plan = build_plan(&[dynamic], &BTreeMap::new(), &policy, 4096).unwrap();
        assert_eq!(plan.pools[0].proposed.process_idle_timeout_seconds, None);
        assert!(
            plan.pools[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("ondemand"))
        );
    }

    proptest::proptest! {
        #[test]
        fn randomized_feasible_plans_respect_budget_and_pool_bounds(
            worker_mb in 1_u32..1024,
            current in 1_u32..200,
            min in 1_u32..20,
            extra in 0_u32..200,
            host_mb in 1_u64..50000,
        ) {
            let max = min.saturating_add(extra).max(min);
            let mut policy = PolicyFile::default();
            policy.global.reserve_memory_mb = 0;
            policy.global.memory_utilization_percent = 100;
            policy.global.default_min_children = min;
            policy.global.default_max_children = max;
            policy.global.default_worker_memory_mb = worker_mb;
            let plan = build_plan(&[pool("www", current)], &BTreeMap::new(), &policy, host_mb).unwrap();
            let decision = &plan.pools[0];
            let proposed = decision.proposed.max_children.unwrap();
            proptest::prop_assert!((min..=max).contains(&proposed));
            if plan.feasible {
                proptest::prop_assert!(plan.allocated_memory_mb <= plan.available_fpm_memory_mb);
            }
        }
    }
}
