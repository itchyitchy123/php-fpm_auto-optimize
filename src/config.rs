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
        if g.default_min_children > g.default_max_children {
            bail!("default child bounds are inverted");
        }
        for (name, p) in &self.pools {
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
