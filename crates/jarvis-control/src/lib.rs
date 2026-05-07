//! Control Plane / Task Plane scaffolding (Section 9.10).
//!
//! The Control Plane must always answer the user. It owns:
//!
//! - the user-facing message channel,
//! - response-time SLAs (Section 9.10.3),
//! - sub-agent watchdogs (Section 9.10.4),
//! - the fallback responder when the Task Plane is slow or down (Section 9.10.6).
//!
//! v0.1 implements this without spawning real worker processes — the
//! "task plane" here is a `tokio::task` that runs the Router. That's
//! enough to exercise the SLA and watchdog primitives end-to-end.

pub mod capacity_advisor;
pub mod control_plane;
pub mod fallback;
pub mod replication;
pub mod scheduler;
pub mod sla;
pub mod tracing_init;
pub mod watchdog;

pub use capacity_advisor::{evaluate as evaluate_capacity, AdvisorPolicy, AdvisoryLevel, ContextHealth};
pub use control_plane::*;
pub use fallback::*;
pub use replication::{ReplicationConfig, ReplicationEnvelope, Replicator};
pub use scheduler::{MaintenanceJobs, Scheduler, SchedulerConfig};
pub use tracing_init::init as init_tracing;
pub use sla::*;
pub use watchdog::*;

#[cfg(test)]
mod tests;
