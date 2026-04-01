pub mod audit;
pub mod benchmark;
pub mod external_power;
pub mod formula;
pub mod inventory;
pub mod memory_policy;
pub mod metrics;
pub mod orchestrator;
pub mod platform;
pub mod processes;
pub mod telemetry;
pub mod workload_catalog;

pub use formula::{FormulaResult, WorkloadProfile};
pub use metrics::ResourceState;
pub use orchestrator::DomeResult;
pub use platform::{PlatformInfo, SoulRamBackendInfo};
pub use telemetry::{EnergyPricing, LifetimeGains, MachineActivity, TelemetrySummary};
