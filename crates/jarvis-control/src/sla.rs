//! Response-time SLAs. Section 9.10.3.

use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct ResponseSla {
    pub interrupt_ack: Duration,
    pub progress_query: Duration,
    pub sub_agent_reply: Duration,
    pub fallback_ack: Duration,
}

impl ResponseSla {
    pub const fn defaults() -> Self {
        Self {
            interrupt_ack: Duration::from_millis(500),
            progress_query: Duration::from_millis(800),
            sub_agent_reply: Duration::from_millis(1000),
            fallback_ack: Duration::from_millis(2000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    InterruptAck,
    ProgressQuery,
    SubAgentReply,
    FallbackAck,
    Normal,
}
