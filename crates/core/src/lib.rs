//! DriveSage core: disk scanning, drive detection & benchmarking,
//! rule-based and AI-assisted cleanup plans, and a safe, undoable executor.
//!
//! The core never deletes files. Every change is a *move* that is written to
//! a journal first, so a whole run can be undone.

pub mod ai;
pub mod bench;
pub mod categories;
pub mod config;
pub mod drives;
pub mod plan;
pub mod protect;
pub mod rules;
pub mod scan;

pub use config::AppConfig;
