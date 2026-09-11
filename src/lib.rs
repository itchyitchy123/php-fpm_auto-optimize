//! Core library for FPM Lens.

pub mod config;
mod fsutil;
pub mod inventory;
pub mod model;
pub mod observe;
pub mod planner;
pub mod render;
pub mod system;
pub mod tui;

pub use config::PolicyFile;
pub use inventory::{discover_pool_dirs, load_inventory};
pub use model::*;
pub use observe::{observe, observe_with_status, observe_with_status_options};
pub use planner::build_plan;
pub mod artifact;
