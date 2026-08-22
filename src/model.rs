use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessManager {
    Static,
    Dynamic,
    Ondemand,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FpmSettings {
    pub pm: ProcessManager,
    pub max_children: Option<u32>,
    pub max_requests: Option<u32>,
    pub process_idle_timeout_seconds: Option<u32>,
    pub request_terminate_timeout_seconds: Option<u32>,
    pub start_servers: Option<u32>,
    pub min_spare_servers: Option<u32>,
    pub max_spare_servers: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolId {
    pub directory: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pool {
    pub id: PoolId,
    pub source_files: Vec<PathBuf>,
    pub settings: FpmSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Evidence {
    pub peak_workers: Option<u32>,
    pub worker_memory_mb: Option<u32>,
    pub saturation_events: u32,
    pub samples: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PoolPolicy {
    pub selected: Option<bool>,
    pub target_children: Option<u32>,
    pub min_children: Option<u32>,
    pub max_children: Option<u32>,
    pub max_requests: Option<u32>,
    pub process_idle_timeout_seconds: Option<u32>,
    pub request_terminate_timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalPolicy {
    pub reserve_memory_mb: u64,
    pub memory_utilization_percent: u8,
    pub default_worker_memory_mb: u32,
    pub minimum_evidence_samples: u32,
    pub headroom_percent: u8,
    pub default_min_children: u32,
    pub default_max_children: u32,
}

impl Default for GlobalPolicy {
    fn default() -> Self {
        Self {
            reserve_memory_mb: 1024,
            memory_utilization_percent: 80,
            default_worker_memory_mb: 64,
            minimum_evidence_samples: 12,
            headroom_percent: 25,
            default_min_children: 2,
            default_max_children: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolDecision {
    pub id: PoolId,
    pub selected: bool,
    pub current: FpmSettings,
    pub proposed: FpmSettings,
    pub minimum_children: u32,
    pub maximum_children: u32,
    pub worker_memory_mb: u32,
    pub evidence: Evidence,
    pub confidence: Confidence,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub generated_at_unix: u64,
    pub host_memory_mb: u64,
    pub available_fpm_memory_mb: u64,
    pub allocated_memory_mb: u64,
    pub feasible: bool,
    pub warnings: Vec<String>,
    pub pools: Vec<PoolDecision>,
}
