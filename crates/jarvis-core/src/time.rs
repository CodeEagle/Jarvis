use chrono::{DateTime, Utc};

pub fn now() -> DateTime<Utc> {
    Utc::now()
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}
