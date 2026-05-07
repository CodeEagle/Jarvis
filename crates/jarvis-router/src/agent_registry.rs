//! Built-in agent registry. Section 8.12.2.

use jarvis_core::agent::{AgentDefinition, RuntimeMode};
use jarvis_core::tool::ToolScope;

pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![
        orchestrator(),
        coding_agent(),
        devops_agent(),
        research_agent(),
        creative_agent(),
        project_manager_agent(),
        memory_agent(),
        verifier_agent(),
        general_agent(),
    ]
}

fn orchestrator() -> AgentDefinition {
    AgentDefinition {
        r#type: "orchestrator".into(),
        version: "0.1".into(),
        display_name: "Jarvis".into(),
        avatar_emoji: "🧠".into(),
        tagline: "统筹任务，协调各助手协作完成复杂目标".into(),
        mentionable: false,
        mention_aliases: vec![],
        trigger_domains: vec!["all".into()],
        trigger_intents: vec![],
        priority: 0,
        preferred_runtime_mode: RuntimeMode::InProcess,
        can_be_sub_agent: false,
        can_be_orchestrator: true,
        capabilities: vec!["调度多 Agent 协作".into()],
        limitations: vec!["不直接执行具体任务".into()],
        default_tool_scope: ToolScope::empty(),
        preferred_model: None,
        token_budget_hint: 8000,
        system_prompt_template: String::new(),
    }
}

fn coding_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "coding".into(),
        version: "0.1".into(),
        display_name: "代码助手".into(),
        avatar_emoji: "💻".into(),
        tagline: "写代码、改代码、看代码".into(),
        mentionable: true,
        mention_aliases: vec![
            "代码".into(),
            "coding".into(),
            "coder".into(),
            "code".into(),
        ],
        trigger_domains: vec!["coding".into()],
        trigger_intents: vec!["coding.".into()],
        priority: 10,
        preferred_runtime_mode: RuntimeMode::WorkerProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec!["代码生成 / 重构 / 调试".into()],
        limitations: vec!["不直接执行 shell 命令".into()],
        default_tool_scope: ToolScope {
            allowed_tools: vec![
                "read_file".into(),
                "list_dir".into(),
                "write_file".into(),
                "create_file".into(),
                "web.search".into(),
            ],
            blocked_tools: vec![],
            requires_confirmation: vec!["delete_file".into(), "shell.exec".into()],
            max_tool_calls: 30,
            max_parallel_tools: 3,
        },
        preferred_model: None,
        token_budget_hint: 12000,
        system_prompt_template: String::new(),
    }
}

fn devops_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "devops".into(),
        version: "0.1".into(),
        display_name: "运维助手".into(),
        avatar_emoji: "🔧".into(),
        tagline: "网络、系统、Docker、路由器".into(),
        mentionable: true,
        mention_aliases: vec![
            "运维".into(),
            "devops".into(),
            "网络".into(),
            "系统".into(),
        ],
        trigger_domains: vec!["devops".into()],
        trigger_intents: vec!["devops.".into()],
        priority: 10,
        preferred_runtime_mode: RuntimeMode::WorkerProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec!["系统配置 / 网络排障".into()],
        limitations: vec!["危险命令必须确认".into()],
        default_tool_scope: ToolScope {
            allowed_tools: vec![
                "read_file".into(),
                "read_config".into(),
                "list_dir".into(),
                "inspect_system".into(),
                "logread".into(),
                "web.search".into(),
                "shell.exec".into(),
            ],
            blocked_tools: vec![],
            requires_confirmation: vec![
                "shell.exec".into(),
                "modify_config".into(),
                "restart_service".into(),
                "modify_firewall".into(),
                "router_config".into(),
            ],
            max_tool_calls: 40,
            max_parallel_tools: 2,
        },
        preferred_model: None,
        token_budget_hint: 12000,
        system_prompt_template: String::new(),
    }
}

fn research_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "research".into(),
        version: "0.1".into(),
        display_name: "研究助手".into(),
        avatar_emoji: "🔍".into(),
        tagline: "搜索、整理、对比".into(),
        mentionable: true,
        mention_aliases: vec![
            "研究".into(),
            "搜索".into(),
            "research".into(),
            "search".into(),
        ],
        trigger_domains: vec!["research".into()],
        trigger_intents: vec!["research.".into()],
        priority: 10,
        preferred_runtime_mode: RuntimeMode::InProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec!["网络搜索 / 文档摘要".into()],
        limitations: vec!["只读".into()],
        default_tool_scope: ToolScope {
            allowed_tools: vec![
                "web.search".into(),
                "web.fetch".into(),
                "read_file".into(),
            ],
            blocked_tools: vec![],
            requires_confirmation: vec![],
            max_tool_calls: 20,
            max_parallel_tools: 4,
        },
        preferred_model: None,
        token_budget_hint: 8000,
        system_prompt_template: String::new(),
    }
}

fn creative_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "creative".into(),
        version: "0.1".into(),
        display_name: "创意助手".into(),
        avatar_emoji: "🎨".into(),
        tagline: "图标、UI、文案、Prompt".into(),
        mentionable: true,
        mention_aliases: vec![
            "创意".into(),
            "设计".into(),
            "creative".into(),
            "design".into(),
        ],
        trigger_domains: vec!["creative".into()],
        trigger_intents: vec!["creative.".into()],
        priority: 10,
        preferred_runtime_mode: RuntimeMode::InProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec!["Prompt 生成 / UI 方案".into()],
        limitations: vec!["不直接生成图片".into()],
        default_tool_scope: ToolScope {
            allowed_tools: vec![
                "web.search".into(),
                "image_gen.prompt".into(),
                "read_file".into(),
                "create_note".into(),
            ],
            blocked_tools: vec![],
            requires_confirmation: vec![],
            max_tool_calls: 15,
            max_parallel_tools: 2,
        },
        preferred_model: None,
        token_budget_hint: 6000,
        system_prompt_template: String::new(),
    }
}

fn project_manager_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "project_manager".into(),
        version: "0.1".into(),
        display_name: "项目助手".into(),
        avatar_emoji: "📋".into(),
        tagline: "拆解目标、跟进进度".into(),
        mentionable: true,
        mention_aliases: vec!["项目".into(), "pm".into(), "project".into()],
        trigger_domains: vec!["project".into()],
        trigger_intents: vec!["project.".into()],
        priority: 5,
        preferred_runtime_mode: RuntimeMode::WorkerProcess,
        can_be_sub_agent: false,
        can_be_orchestrator: true,
        capabilities: vec!["任务拆解 / 进度跟踪".into()],
        limitations: vec!["不替代用户做最终决策".into()],
        default_tool_scope: ToolScope {
            allowed_tools: vec![
                "read_file".into(),
                "create_note".into(),
                "web.search".into(),
            ],
            blocked_tools: vec![],
            requires_confirmation: vec![],
            max_tool_calls: 10,
            max_parallel_tools: 1,
        },
        preferred_model: None,
        token_budget_hint: 6000,
        system_prompt_template: String::new(),
    }
}

fn memory_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "memory".into(),
        version: "0.1".into(),
        display_name: "记忆助手".into(),
        avatar_emoji: "🗂️".into(),
        tagline: "管理长期偏好和事实".into(),
        mentionable: false,
        mention_aliases: vec![],
        trigger_domains: vec!["memory".into()],
        trigger_intents: vec!["memory.".into()],
        priority: 100,
        preferred_runtime_mode: RuntimeMode::InProcess,
        can_be_sub_agent: false,
        can_be_orchestrator: false,
        capabilities: vec!["记录 / 查询 / 清理".into()],
        limitations: vec!["不主动推断敏感信息".into()],
        default_tool_scope: ToolScope::empty(),
        preferred_model: None,
        token_budget_hint: 4000,
        system_prompt_template: String::new(),
    }
}

fn verifier_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "verifier".into(),
        version: "0.1".into(),
        display_name: "核查助手".into(),
        avatar_emoji: "🔍✓".into(),
        tagline: "独立验证产出".into(),
        mentionable: false,
        mention_aliases: vec![],
        trigger_domains: vec!["internal".into()],
        trigger_intents: vec![],
        priority: 0,
        preferred_runtime_mode: RuntimeMode::WorkerProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec!["验证文件 / 重跑测试".into()],
        limitations: vec!["只读".into()],
        default_tool_scope: ToolScope {
            allowed_tools: vec![
                "read_file".into(),
                "list_dir".into(),
                "shell.exec".into(),
            ],
            blocked_tools: vec![],
            requires_confirmation: vec![],
            max_tool_calls: 20,
            max_parallel_tools: 2,
        },
        preferred_model: None,
        token_budget_hint: 6000,
        system_prompt_template: String::new(),
    }
}

fn general_agent() -> AgentDefinition {
    AgentDefinition {
        r#type: "general".into(),
        version: "0.1".into(),
        display_name: "通用助手".into(),
        avatar_emoji: "💬".into(),
        tagline: "兜底问答".into(),
        mentionable: false,
        mention_aliases: vec![],
        trigger_domains: vec!["chat".into()],
        trigger_intents: vec![],
        priority: 1,
        preferred_runtime_mode: RuntimeMode::InProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec!["普通问答".into()],
        limitations: vec!["无专项工具".into()],
        default_tool_scope: ToolScope::empty(),
        preferred_model: None,
        token_budget_hint: 4000,
        system_prompt_template: String::new(),
    }
}

/// Match agent by intent + domain. Section 8.12.3.
pub fn match_agent<'a>(
    intent: &str,
    domain: &str,
    registry: &'a [AgentDefinition],
) -> &'a AgentDefinition {
    let candidates: Vec<&AgentDefinition> = registry
        .iter()
        .filter(|a| a.can_be_sub_agent || a.can_be_orchestrator)
        .filter(|a| {
            a.trigger_domains.iter().any(|d| d == domain || d == "all")
                && (a.trigger_intents.is_empty()
                    || a.trigger_intents.iter().any(|p| intent.starts_with(p)))
        })
        .collect();
    let chosen = candidates.into_iter().max_by_key(|a| a.priority);
    chosen.unwrap_or_else(|| {
        registry
            .iter()
            .find(|a| a.r#type == "general")
            .expect("general agent must exist")
    })
}

/// Resolve an `@`-mention name (display_name / type / alias) to the
/// agent definition. Only `mentionable = true` agents are considered.
/// PRD §8.12.3.
pub fn match_agent_by_mention<'a>(
    name: &str,
    registry: &'a [AgentDefinition],
) -> Option<&'a AgentDefinition> {
    registry.iter().find(|a| a.matches_mention(name))
}

/// Every agent the user is allowed to `@`. Useful for the unresolved-
/// mention warning the router emits and the help screen the macOS
/// composer shows. PRD §8.12.3.
pub fn get_mentionable_agents(registry: &[AgentDefinition]) -> Vec<&AgentDefinition> {
    registry.iter().filter(|a| a.is_mentionable()).collect()
}
