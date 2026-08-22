use crate::model::{GlobalPolicy, PoolPolicy};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PolicyFile {
    pub global: GlobalPolicy,
    pub pools: BTreeMap<String, PoolPolicy>,
}

impl PolicyFile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("could not read policy {}", path.display()))?;
        if text.len() > 1024 * 1024 {
            bail!("policy {} exceeds 1 MiB", path.display());
        }
        let policy: Self =
            toml::from_str(&text).with_context(|| format!("invalid policy {}", path.display()))?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        let g = &self.global;
        if !(1..=100).contains(&g.memory_utilization_percent) {
            bail!("memory_utilization_percent must be 1..=100");
        }
        if g.default_worker_memory_mb == 0 {
            bail!("default_worker_memory_mb must be positive");
        }
        if g.minimum_evidence_samples == 0 {
            bail!("minimum_evidence_samples must be positive");
        }
        if g.default_min_children == 0 || g.default_max_children == 0 {
            bail!("default child bounds must be positive");
        }
        if !(1..=100).contains(&g.minimum_status_success_percent) {
            bail!("minimum_status_success_percent must be 1..=100");
        }
        if g.maximum_evidence_age_seconds == 0 || g.minimum_observation_seconds == 0 {
            bail!("evidence age and observation duration limits must be positive");
        }
        if g.default_min_children > g.default_max_children {
            bail!("default child bounds are inverted");
        }
        for (name, p) in &self.pools {
            if p.min_children == Some(0)
                || p.max_children == Some(0)
                || p.target_children == Some(0)
            {
                bail!("pool {name} child limits must be positive");
            }
            if p.min_children
                .zip(p.max_children)
                .is_some_and(|(a, b)| a > b)
            {
                bail!("pool {name} has min_children greater than max_children");
            }
            if let Some(target) = p.target_children {
                if p.min_children.is_some_and(|v| target < v)
                    || p.max_children.is_some_and(|v| target > v)
                {
                    bail!("pool {name} target_children is outside its bounds");
                }
            }
        }
        Ok(())
    }

    pub fn for_pool(&self, name: &str, qualified: &str) -> PoolPolicy {
        self.pools
            .get(qualified)
            .or_else(|| self.pools.get(name))
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_children_and_evidence_thresholds() {
        let mut policy = PolicyFile::default();
        policy.global.default_min_children = 0;
        assert!(policy.validate().is_err());
        let mut policy = PolicyFile::default();
        policy.global.minimum_evidence_samples = 0;
        assert!(policy.validate().is_err());
    }
}
