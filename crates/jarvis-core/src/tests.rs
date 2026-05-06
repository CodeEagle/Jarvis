#[cfg(test)]
use crate::*;

#[test]
fn id_prefixes_are_correct() {
    assert!(new_trace_id().starts_with("trc_"));
    assert!(new_task_id().starts_with("task_"));
    assert!(new_session_id().starts_with("sess_"));
    assert!(new_memory_id().starts_with("mem_"));
}

#[test]
fn id_uniqueness() {
    let a = new_trace_id();
    let b = new_trace_id();
    assert_ne!(a, b);
}

#[test]
fn domain_parses_l1_from_intent() {
    assert_eq!(Domain::parse_l1("coding.debug"), Some(Domain::Coding));
    assert_eq!(
        Domain::parse_l1("devops.networking"),
        Some(Domain::Devops)
    );
    assert_eq!(Domain::parse_l1("memory.update"), Some(Domain::Memory));
    assert_eq!(Domain::parse_l1("nonsense"), None);
}

#[test]
fn memory_type_half_life_matches_spec() {
    assert_eq!(MemoryType::LessonMemory.half_life_days(), 90);
    assert_eq!(MemoryType::PreferenceMemory.half_life_days(), 60);
    assert_eq!(MemoryType::EpisodeMemory.half_life_days(), 14);
    assert_eq!(MemoryType::ScratchMemory.half_life_days(), 0);
}

#[test]
fn emotion_gate_only_supports_emotion_for_specific_types() {
    // Section 12.3a: emotion only applies to episode/lesson/cluster/inference.
    assert!(MemoryType::EpisodeMemory.supports_emotion());
    assert!(MemoryType::LessonMemory.supports_emotion());
    assert!(MemoryType::ClusterMemory.supports_emotion());
    assert!(MemoryType::InferenceMemory.supports_emotion());
    // Others should not.
    assert!(!MemoryType::FactMemory.supports_emotion());
    assert!(!MemoryType::PreferenceMemory.supports_emotion());
    assert!(!MemoryType::ProjectMemory.supports_emotion());
    assert!(!MemoryType::EntityMemory.supports_emotion());
    assert!(!MemoryType::RelationMemory.supports_emotion());
    assert!(!MemoryType::ScratchMemory.supports_emotion());
}

#[test]
fn fact_memory_emotion_gate_forces_neutral() {
    let now = now();
    let mem = Memory {
        id: "m1".into(),
        r#type: MemoryType::FactMemory,
        scope: "global".into(),
        content: "Flutter SDK 3.19".into(),
        entities: vec!["Flutter".into()],
        confidence: 0.9,
        trust_score: 0.9,
        half_life_days: 30,
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::TaskResult,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 7.0,
        emotion_polarity: EmotionPolarity::Positive,
        tier: 2,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: now,
        updated_at: now,
    };
    let gated = mem.enforce_emotion_gate();
    assert_eq!(gated.emotion_energy, 0.0);
    assert_eq!(gated.emotion_polarity, EmotionPolarity::Neutral);
}

#[test]
fn episode_memory_emotion_gate_keeps_values() {
    let now = now();
    let mem = Memory {
        id: "m1".into(),
        r#type: MemoryType::EpisodeMemory,
        scope: "sess_1".into(),
        content: "用户重构完成后非常满意".into(),
        entities: vec![],
        confidence: 0.7,
        trust_score: 0.7,
        half_life_days: 14,
        retrieve_count: 0,
        last_retrieved_at: None,
        source_trace_id: None,
        source_type: SourceType::TaskResult,
        conflict_ids: vec![],
        status: MemoryStatus::Approved,
        emotion_energy: 8.0,
        emotion_polarity: EmotionPolarity::Positive,
        tier: 3,
        expires_at: None,
        cluster_member_ids: vec![],
        created_at: now,
        updated_at: now,
    };
    let gated = mem.enforce_emotion_gate();
    assert_eq!(gated.emotion_energy, 8.0);
    assert_eq!(gated.emotion_polarity, EmotionPolarity::Positive);
}

#[test]
fn source_type_default_confidence() {
    // Section 12.3
    assert!((SourceType::UserExplicit.default_confidence() - 0.95).abs() < 1e-6);
    assert!((SourceType::Correction.default_confidence() - 0.90).abs() < 1e-6);
    assert!((SourceType::TaskResult.default_confidence() - 0.70).abs() < 1e-6);
}

#[test]
fn tool_scope_blocked_overrides_allowed() {
    let scope = ToolScope {
        allowed_tools: vec!["shell.exec".into()],
        blocked_tools: vec!["shell.exec".into()],
        requires_confirmation: vec![],
        max_tool_calls: 1,
        max_parallel_tools: 1,
    };
    assert!(!scope.is_allowed("shell.exec"));
}

#[test]
fn tool_scope_allows_listed_tool() {
    let scope = ToolScope {
        allowed_tools: vec!["read_file".into()],
        blocked_tools: vec![],
        requires_confirmation: vec![],
        max_tool_calls: 5,
        max_parallel_tools: 1,
    };
    assert!(scope.is_allowed("read_file"));
    assert!(!scope.is_allowed("shell.exec"));
}

#[test]
fn tool_scope_requires_confirmation() {
    let scope = ToolScope {
        allowed_tools: vec!["shell.exec".into()],
        blocked_tools: vec![],
        requires_confirmation: vec!["shell.exec".into()],
        max_tool_calls: 1,
        max_parallel_tools: 1,
    };
    assert!(scope.is_allowed("shell.exec"));
    assert!(scope.requires_confirmation("shell.exec"));
    assert!(!scope.requires_confirmation("read_file"));
}

#[test]
fn agent_definition_only_matches_mention_when_mentionable() {
    let def = AgentDefinition {
        r#type: "coding".into(),
        version: "1".into(),
        display_name: "代码助手".into(),
        avatar_emoji: "💻".into(),
        tagline: "".into(),
        mentionable: true,
        mention_aliases: vec!["coding".into(), "代码".into()],
        trigger_domains: vec!["coding".into()],
        trigger_intents: vec![],
        priority: 1,
        preferred_runtime_mode: RuntimeMode::WorkerProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec![],
        limitations: vec![],
        default_tool_scope: ToolScope::empty(),
        preferred_model: None,
        token_budget_hint: 0,
        system_prompt_template: String::new(),
    };
    assert!(def.matches_mention("代码助手"));
    assert!(def.matches_mention("代码"));
    assert!(def.matches_mention("coding"));
    assert!(!def.matches_mention("foo"));
}

#[test]
fn internal_agents_are_not_mentionable() {
    let def = AgentDefinition {
        r#type: "verifier".into(),
        version: "1".into(),
        display_name: "核查助手".into(),
        avatar_emoji: "🔍".into(),
        tagline: "".into(),
        mentionable: false,
        mention_aliases: vec!["核查".into()],
        trigger_domains: vec!["internal".into()],
        trigger_intents: vec![],
        priority: 0,
        preferred_runtime_mode: RuntimeMode::WorkerProcess,
        can_be_sub_agent: true,
        can_be_orchestrator: false,
        capabilities: vec![],
        limitations: vec![],
        default_tool_scope: ToolScope::empty(),
        preferred_model: None,
        token_budget_hint: 0,
        system_prompt_template: String::new(),
    };
    // Even if name matches, mentionable=false means @ shouldn't resolve it.
    assert!(!def.matches_mention("核查助手"));
    assert!(!def.matches_mention("核查"));
}
