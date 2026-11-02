//! Library detection, scanning, metadata extraction and thumbnails.
//!
//! All disk and CPU heavy work runs on the engine's own thread and runtime
//! (see [`engine`]); the API only reads progress snapshots.

pub mod admin;
pub mod analyze;
pub mod dates;
pub mod detect;
pub mod engine;
pub mod faces;
pub mod kinds;
pub mod marker;
pub mod scan;
pub mod thumbs;
pub mod tools;

pub use engine::{EngineConfig, EngineHandle, EngineSnapshot, JobProgress};
