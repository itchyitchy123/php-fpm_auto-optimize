//! Core library for FPM Lens.

pub mod config;
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
pub use observe::observe;
pub use planner::build_plan;
