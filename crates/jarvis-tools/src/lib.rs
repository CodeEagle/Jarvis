//! Tool Runtime (Section 11).
//!
//! Tool calls flow through this crate. The runtime:
//!
//! 1. Validates the call against the active `ToolScope`
//!    (allow / block / requires_confirmation).
//! 2. Records every call to `raw_event_log` (Section 30.6).
//! 3. Returns a structured `ToolResult` so callers can handle timeouts,
//!    permission denials, and unavailability uniformly (Section 11.4).
//!
//! v0.1 ships a stub registry: `read_file` and `web.search` use safe
//! mocks so the runtime is fully testable without real I/O. Real tool
//! adapters land in v0.2 / v0.3.

pub mod registry;
pub mod result;
pub mod runtime;

pub use registry::{Tool, ToolBox, ToolRegistry};
pub use result::{ToolResult, ToolStatus};
pub use runtime::{CallContext, ToolRuntime};

#[cfg(test)]
mod tests;
