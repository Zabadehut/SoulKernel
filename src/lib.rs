//! SoulKernel library — Performance Dome orchestrator.
//! Re-exports for use as a dependency or tests.

pub mod external_power;
pub mod formula;
pub mod memory_policy;
pub mod metrics;
pub mod orchestrator;
pub mod platform;
pub mod telemetry;
pub mod workload_catalog;

pub use formula::{FormulaResult, WorkloadProfile};
pub use metrics::ResourceState;
pub use orchestrator::DomeResult;
pub use platform::PlatformInfo;
pub use telemetry::{EnergyPricing, LifetimeGains, MachineActivity, TelemetrySummary};
