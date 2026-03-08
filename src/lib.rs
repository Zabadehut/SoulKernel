//! SoulKernel library — Performance Dome orchestrator.
//! Re-exports for use as a dependency or tests.

pub mod formula;
pub mod metrics;
pub mod orchestrator;
pub mod platform;

pub use formula::{FormulaResult, WorkloadProfile};
pub use metrics::ResourceState;
pub use orchestrator::DomeResult;
pub use platform::PlatformInfo;
