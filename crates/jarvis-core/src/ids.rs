use uuid::Uuid;

pub fn new_trace_id() -> String {
    format!("trc_{}", short_uuid())
}

pub fn new_task_id() -> String {
    format!("task_{}", short_uuid())
}

pub fn new_session_id() -> String {
    format!("sess_{}", short_uuid())
}

pub fn new_message_id() -> String {
    format!("msg_{}", short_uuid())
}

pub fn new_memory_id() -> String {
    format!("mem_{}", short_uuid())
}

pub fn new_agent_id() -> String {
    format!("agent_{}", short_uuid())
}

pub fn new_artifact_id() -> String {
    format!("artf_{}", short_uuid())
}

pub fn new_id_with_prefix(prefix: &str) -> String {
    format!("{}_{}", prefix, short_uuid())
}

fn short_uuid() -> String {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}
