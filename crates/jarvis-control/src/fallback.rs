//! Fallback responder. Section 9.10.3 / 9.10.6.
//!
//! When the Task Plane has not produced a reply within `fallback_ack`,
//! the Control Plane delivers a short, honest holding message. The
//! function is pure so we can unit-test it.

#[derive(Debug, Clone, Copy)]
pub enum TaskPlaneState {
    Idle,
    Executing,
    Stuck,
    Unavailable,
}

pub fn fallback_message(state: TaskPlaneState, current_agent: Option<&str>) -> String {
    match state {
        TaskPlaneState::Idle => "收到，让我看看…".into(),
        TaskPlaneState::Executing => match current_agent {
            Some(a) => format!("正在后台处理，{a} 还在跑，有什么需要我帮你的吗？"),
            None => "正在后台处理，有什么需要我帮你的吗？".into(),
        },
        TaskPlaneState::Stuck => "后台任务似乎遇到了问题，我来检查一下…".into(),
        TaskPlaneState::Unavailable => {
            "当前系统处于轻量模式，复杂任务会在恢复后自动继续。".into()
        }
    }
}
