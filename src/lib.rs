//! zevbox: run ACP agents for Zed inside Apple Container micro-VMs.
//!
//! The implementation contract is `zevbox_plan.md`. Section numbers (§) in doc
//! comments refer to that document.

pub mod agents;
pub mod cli;
pub mod config;
pub mod container;
pub mod fsutil;
pub mod guard;
pub mod instances;
pub mod omlx;
pub mod paths;
pub mod relay;
pub mod release;
pub mod supervisor;
pub mod tools;
pub mod ui;
pub mod zed_settings;
