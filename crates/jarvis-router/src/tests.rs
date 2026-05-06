#[cfg(test)]
use crate::*;
#[cfg(test)]
use chrono::Duration;
#[cfg(test)]
use jarvis_core::route::SessionAction;
#[cfg(test)]
use jarvis_core::session::{Session, SessionStatus};
#[cfg(test)]
use jarvis_core::time::now;
#[cfg(test)]
use jarvis_db::Db;

// ── intent rules (Section 30.1.1) ───────────────────────────────────────

#[test]
fn devops_keyword_emits_devops_networking_hint() {
    let hints = intent::apply_rules("OpenWrt hosts 解析不生效");
    let names: Vec<&str> = hints.iter().map(|h| h.hint.as_str()).collect();
    assert!(names.contains(&"devops.networking"));
}

#[test]
fn coding_keyword_emits_coding_debug_hint() {
    let hints = intent::apply_rules("这个报错帮我看看");
    let names: Vec<&str> = hints.iter().map(|h| h.hint.as_str()).collect();
    assert!(names.contains(&"coding.debug"));
}

#[test]
fn memory_update_keyword_emits_memory_hint() {
    let hints = intent::apply_rules("记住我不喜欢用 class component");
    let names: Vec<&str> = hints.iter().map(|h| h.hint.as_str()).collect();
    assert!(names.contains(&"memory.update"));
}

#[test]
fn creative_rule_uses_creative_icon_not_design() {
    // Section 30.1.1: regression check that we removed `creative.design`.
    let hints = intent::apply_rules("帮我生成一个图标 prompt");
    let names: Vec<&str> = hints.iter().map(|h| h.hint.as_str()).collect();
    assert!(names.contains(&"creative.icon"));
    assert!(!names.contains(&"creative.design"));
}

#[test]
fn no_rule_match_returns_empty() {
    let hints = intent::apply_rules("今天天气怎么样");
    assert!(hints.is_empty());
}

// ── session resolver (Section 30.1.2) ───────────────────────────────────

#[test]
fn score_uses_correct_weights() {
    let s = session_resolver::score(&session_resolver::SessionScoreInputs {
        semantic_similarity: 0.8,
        recency_score: 0.9,
        entity_overlap: 0.7,
    });
    let expected = 0.8 * 0.40 + 0.9 * 0.30 + 0.7 * 0.30;
    assert!((s - expected).abs() < 1e-5);
}

#[test]
fn score_above_threshold_continues() {
    let d = session_resolver::decide(Some("sess_1"), 0.80);
    match d {
        session_resolver::SessionDecision::ContinueExisting {
            clarification_needed,
            ..
        } => assert!(!clarification_needed),
        other => panic!("expected continue, got {other:?}"),
    }
}

#[test]
fn score_below_low_threshold_creates_new() {
    let d = session_resolver::decide(Some("sess_1"), 0.40);
    assert_eq!(d, session_resolver::SessionDecision::CreateNew);
}

#[test]
fn score_in_band_continues_with_clarification() {
    let d = session_resolver::decide(Some("sess_1"), 0.60);
    match d {
        session_resolver::SessionDecision::ContinueExisting {
            clarification_needed,
            ..
        } => assert!(clarification_needed),
        other => panic!("expected continue with clarification, got {other:?}"),
    }
}

#[test]
fn explicit_reference_detection() {
    assert!(session_resolver::has_explicit_reference("继续刚才那个"));
    assert!(session_resolver::has_explicit_reference("继续"));
    assert!(!session_resolver::has_explicit_reference("帮我重构"));
}

#[test]
fn recency_decays_to_zero_after_two_weeks() {
    let n = now();
    let fresh = session_resolver::recency_score(n, n);
    let mid = session_resolver::recency_score(n - Duration::days(7), n);
    let old = session_resolver::recency_score(n - Duration::days(30), n);
    assert!((fresh - 1.0).abs() < 1e-6);
    assert!(mid > 0.0 && mid < 1.0);
    assert_eq!(old, 0.0);
}

// ── @mention parsing (Section 30.15.1) ──────────────────────────────────

fn registry() -> Vec<jarvis_core::agent::AgentDefinition> {
    agent_registry::builtin_agents()
}

#[test]
fn no_mention_returns_none_mode() {
    let r = mention::parse_mentions("帮我重构 sync 模块", &registry());
    assert_eq!(r.mention_mode, jarvis_core::mention::MentionMode::None);
    assert_eq!(r.clean_input, "帮我重构 sync 模块");
    assert!(r.mentions.is_empty());
}

#[test]
fn single_mention_resolves_by_display_name() {
    let r = mention::parse_mentions("@代码助手 帮我重构", &registry());
    assert_eq!(r.mention_mode, jarvis_core::mention::MentionMode::Single);
    assert_eq!(r.mentions[0].resolved_type.as_deref(), Some("coding"));
    assert_eq!(r.clean_input, "帮我重构");
}

#[test]
fn alias_mention_resolves() {
    let r = mention::parse_mentions("@coding 帮我", &registry());
    assert_eq!(r.mentions[0].resolved_type.as_deref(), Some("coding"));
}

#[test]
fn multi_mention_returns_multi_mode() {
    let r = mention::parse_mentions("@代码助手 @研究助手 重构并搜索方案", &registry());
    assert_eq!(r.mention_mode, jarvis_core::mention::MentionMode::Multi);
    let resolved: Vec<&str> = r
        .mentions
        .iter()
        .filter_map(|m| m.resolved_type.as_deref())
        .collect();
    assert!(resolved.contains(&"coding"));
    assert!(resolved.contains(&"research"));
}

#[test]
fn unknown_agent_is_unresolved() {
    let r = mention::parse_mentions("@测试助手 帮我跑测试", &registry());
    assert!(r.mentions[0].unresolved);
    assert_eq!(r.mention_mode, jarvis_core::mention::MentionMode::None);
}

#[test]
fn jarvis_orchestrator_is_not_mentionable() {
    let r = mention::parse_mentions("@Jarvis 帮我", &registry());
    assert!(r.mentions[0].unresolved);
}

#[test]
fn verifier_is_not_mentionable() {
    let r = mention::parse_mentions("@核查助手 验证一下", &registry());
    assert!(r.mentions[0].unresolved);
}

// ── Router integration ──────────────────────────────────────────────────

fn fresh_db() -> Db {
    Db::in_memory().unwrap()
}

#[test]
fn route_writes_raw_event_log_first() {
    let db = fresh_db();
    let r = Router::new(db.clone());
    let (_decision, diag) = r
        .route(RouterInput {
            user_input: "OpenWrt hosts 解析问题",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    assert!(diag.raw_event_seq >= 1);
    let evt = jarvis_db::raw_event_log::get(&db, diag.raw_event_seq).unwrap();
    assert_eq!(evt.event_type, "user_message");
    assert_eq!(evt.raw_content, "OpenWrt hosts 解析问题");
    assert!(evt.verify(), "checksum should validate");
}

#[test]
fn route_devops_input_picks_devops_agent() {
    let db = fresh_db();
    let r = Router::new(db);
    let (decision, _) = r
        .route(RouterInput {
            user_input: "OpenWrt DNS hosts 文件不生效",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    assert_eq!(decision.agent_type, "devops");
    assert_eq!(decision.domain, "devops");
}

#[test]
fn route_mention_overrides_agent_type() {
    let db = fresh_db();
    let r = Router::new(db);
    // input looks like devops, but @ specifies coding
    let (decision, _) = r
        .route(RouterInput {
            user_input: "@代码助手 帮我看看这个 openwrt 相关的代码",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    assert_eq!(decision.agent_type, "coding");
    assert!(decision.mention_override);
    assert!(decision.router_notes.contains("user @ specified"));
}

#[test]
fn route_multi_mention_forces_orchestrator() {
    let db = fresh_db();
    let r = Router::new(db);
    let (decision, _) = r
        .route(RouterInput {
            user_input: "@代码助手 @研究助手 重构并搜索方案",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    assert_eq!(decision.agent_type, "orchestrator");
    assert!(decision.forced_sub_agents.contains(&"coding".to_string()));
    assert!(decision.forced_sub_agents.contains(&"research".to_string()));
    assert!(decision.mention_override);
}

#[test]
fn route_steer_when_mentioned_agent_running() {
    let db = fresh_db();
    let r = Router::new(db);
    let running = vec!["coding".to_string()];
    let (decision, _) = r
        .route(RouterInput {
            user_input: "@代码助手 记得保持 stream 接口不变",
            session_id_hint: None,
            running_agent_types: &running,
        })
        .unwrap();
    assert_eq!(decision.override_action.as_deref(), Some("steer"));
    assert!(decision.steer_content.is_some());
}

#[test]
fn route_unresolved_mention_continues_normally() {
    let db = fresh_db();
    let r = Router::new(db);
    let (decision, diag) = r
        .route(RouterInput {
            user_input: "@测试助手 帮我跑单元测试",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    // Mention can't be resolved but routing must still complete.
    assert!(!decision.agent_type.is_empty());
    assert!(diag.mention_warning.is_some());
    let warn = diag.mention_warning.unwrap();
    assert!(warn.contains("@代码助手") || warn.contains("@运维助手"));
}

#[test]
fn route_explicit_reference_continues_existing_session() {
    let db = fresh_db();
    let n = now();
    jarvis_db::session_repo::upsert_session(
        &db,
        &Session {
            id: "sess_old".into(),
            title: "old".into(),
            domain: "coding".into(),
            topic: "sync 重构".into(),
            summary: "正在重构 sync".into(),
            long_summary: "已完成 sync_state.dart, 还差 sync_executor.dart".into(),
            active_entities: vec!["sync".into()],
            unresolved: vec!["完成 sync_executor.dart".into()],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: SessionStatus::Active,
            created_at: n,
            updated_at: n,
            last_active_at: n,
        },
    )
    .unwrap();

    let r = Router::new(db);
    let (decision, _) = r
        .route(RouterInput {
            user_input: "继续刚才的重构",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    assert_eq!(decision.session_action, SessionAction::ContinueExisting);
    assert_eq!(decision.target_session_id.as_deref(), Some("sess_old"));
}

#[test]
fn confidence_never_reaches_one() {
    let db = fresh_db();
    let r = Router::new(db);
    let (decision, _) = r
        .route(RouterInput {
            user_input: "OpenWrt DNS 报错",
            session_id_hint: None,
            running_agent_types: &[],
        })
        .unwrap();
    assert!(decision.confidence < 1.0);
}

// ── LLM judge (rule-based fallback) ─────────────────────────────────────

#[tokio::test]
async fn rule_based_judge_uses_dominant_hint_for_intent() {
    let judge = llm_judge::RuleBasedJudge;
    let hints = intent::apply_rules("OpenWrt DNS hosts 报错");
    let agents: Vec<String> = registry()
        .iter()
        .filter(|a| a.is_mentionable() || a.r#type == "general")
        .map(|a| a.r#type.clone())
        .collect();
    let outcome = judge
        .judge(llm_judge::JudgeInputs {
            user_input: "OpenWrt DNS hosts 报错",
            trace_id: "trc",
            task_id: "task",
            rule_hints: &hints,
            recent_session_titles: &[],
            allowed_agents: &agents,
        })
        .await
        .expect("judge should always return for fallback adapter");
    assert!(outcome.primary_intent.starts_with("devops")
        || outcome.primary_intent.starts_with("coding"));
    assert!(outcome.confidence < 1.0);
}

#[tokio::test]
async fn rule_based_judge_creates_new_session_when_no_recent() {
    let judge = llm_judge::RuleBasedJudge;
    let hints = intent::apply_rules("帮我搜索 Riverpod 教程");
    let agents = vec!["general".to_string(), "research".to_string()];
    let outcome = judge
        .judge(llm_judge::JudgeInputs {
            user_input: "帮我搜索 Riverpod 教程",
            trace_id: "trc",
            task_id: "task",
            rule_hints: &hints,
            recent_session_titles: &[],
            allowed_agents: &agents,
        })
        .await
        .unwrap();
    assert_eq!(
        outcome.session_action,
        jarvis_core::route::SessionAction::CreateNew
    );
}

#[tokio::test]
async fn rule_based_judge_marks_clarification_when_hints_weak() {
    let judge = llm_judge::RuleBasedJudge;
    let agents = vec!["general".to_string()];
    let outcome = judge
        .judge(llm_judge::JudgeInputs {
            user_input: "帮我看看",
            trace_id: "trc",
            task_id: "task",
            rule_hints: &[],
            recent_session_titles: &[],
            allowed_agents: &agents,
        })
        .await
        .unwrap();
    assert!(outcome.clarification_needed);
}

// ── cold-start integration ──────────────────────────────────────────────

#[test]
fn cold_start_captured_when_gap_exceeds_threshold() {
    let db = fresh_db();
    let n = now();
    jarvis_db::session_repo::upsert_session(
        &db,
        &Session {
            id: "sess_cold".into(),
            title: "test".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "rolling summary v1".into(),
            active_entities: vec![],
            unresolved: vec![],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: SessionStatus::Active,
            created_at: n - Duration::hours(2),
            updated_at: n - Duration::hours(2),
            last_active_at: n - Duration::hours(2),
        },
    )
    .unwrap();

    let r = Router::new(db.clone());
    let id = r
        .maybe_capture_cold_start(
            "sess_cold",
            Duration::minutes(30),
            5,
        )
        .unwrap();
    assert!(id.is_some(), "expected snapshot to be captured");

    // Second call sees the active snapshot and skips.
    let id2 = r
        .maybe_capture_cold_start(
            "sess_cold",
            Duration::minutes(30),
            5,
        )
        .unwrap();
    assert!(id2.is_none(), "expected no second snapshot");
}

#[test]
fn cold_start_skipped_when_session_recent() {
    let db = fresh_db();
    let n = now();
    jarvis_db::session_repo::upsert_session(
        &db,
        &Session {
            id: "sess_recent".into(),
            title: "test".into(),
            domain: "coding".into(),
            topic: "x".into(),
            summary: "".into(),
            long_summary: "".into(),
            active_entities: vec![],
            unresolved: vec![],
            resolved: vec![],
            recent_message_ids: vec![],
            memory_refs: vec![],
            skill_refs: vec![],
            status: SessionStatus::Active,
            created_at: n,
            updated_at: n,
            last_active_at: n,
        },
    )
    .unwrap();

    let r = Router::new(db);
    let id = r
        .maybe_capture_cold_start(
            "sess_recent",
            Duration::minutes(30),
            5,
        )
        .unwrap();
    assert!(id.is_none());
}

// ── Router with LLM judge integration ───────────────────────────────────

#[tokio::test]
async fn route_with_judge_uses_judge_when_more_confident() {
    let db = fresh_db();
    let router = Router::new(db);
    // Custom judge that always returns very high confidence.
    struct AlwaysConfident;
    #[async_trait::async_trait]
    impl llm_judge::LlmJudge for AlwaysConfident {
        async fn judge(
            &self,
            _inputs: llm_judge::JudgeInputs<'_>,
        ) -> Option<llm_judge::JudgeOutcome> {
            Some(llm_judge::JudgeOutcome {
                primary_intent: "research.web_search".into(),
                secondary_intents: vec![],
                domain: "research".into(),
                topic: "判官接管".into(),
                session_action: jarvis_core::route::SessionAction::CreateNew,
                agent_type: "research".into(),
                confidence: 0.99,
                clarification_needed: false,
                router_notes: "judge ran".into(),
            })
        }
    }
    let (decision, _) = router
        .route_with_judge(
            RouterInput {
                user_input: "openwrt 报错",
                session_id_hint: None,
                running_agent_types: &[],
            },
            &AlwaysConfident,
        )
        .await
        .unwrap();
    assert_eq!(decision.agent_type, "research");
    assert!(decision.router_notes.contains("judge ran"));
    assert!(!decision.fallback_used);
}

#[tokio::test]
async fn route_with_judge_marks_fallback_when_judge_returns_none() {
    let db = fresh_db();
    let router = Router::new(db);
    struct NeverAvailable;
    #[async_trait::async_trait]
    impl llm_judge::LlmJudge for NeverAvailable {
        async fn judge(
            &self,
            _inputs: llm_judge::JudgeInputs<'_>,
        ) -> Option<llm_judge::JudgeOutcome> {
            None
        }
    }
    let (decision, _) = router
        .route_with_judge(
            RouterInput {
                user_input: "openwrt 报错",
                session_id_hint: None,
                running_agent_types: &[],
            },
            &NeverAvailable,
        )
        .await
        .unwrap();
    assert!(decision.fallback_used);
}

#[tokio::test]
async fn route_with_judge_respects_mention_override() {
    let db = fresh_db();
    let router = Router::new(db);
    struct AlwaysConfident;
    #[async_trait::async_trait]
    impl llm_judge::LlmJudge for AlwaysConfident {
        async fn judge(
            &self,
            _inputs: llm_judge::JudgeInputs<'_>,
        ) -> Option<llm_judge::JudgeOutcome> {
            Some(llm_judge::JudgeOutcome {
                primary_intent: "research.web_search".into(),
                secondary_intents: vec![],
                domain: "research".into(),
                topic: "x".into(),
                session_action: jarvis_core::route::SessionAction::CreateNew,
                agent_type: "research".into(),
                confidence: 0.99,
                clarification_needed: false,
                router_notes: "judge ran".into(),
            })
        }
    }
    // The user explicitly @ specifies coding — judge should NOT override.
    let (decision, _) = router
        .route_with_judge(
            RouterInput {
                user_input: "@代码助手 帮我重构这个",
                session_id_hint: None,
                running_agent_types: &[],
            },
            &AlwaysConfident,
        )
        .await
        .unwrap();
    assert_eq!(decision.agent_type, "coding");
    assert!(decision.mention_override);
}

// ── system prompt assembler ─────────────────────────────────────────────

#[test]
fn stable_block_includes_agent_identity() {
    let agents = registry();
    let coding = agents.iter().find(|a| a.r#type == "coding").unwrap();
    let block = render_stable_block(&StablePromptInputs {
        agent: coding,
        persona: None,
        framework_directives: &[],
    });
    assert!(block.contains("代码助手"));
    assert!(block.contains("💻"));
}

#[test]
fn stable_block_appends_persona_and_user_md() {
    let agents = registry();
    let coding = agents.iter().find(|a| a.r#type == "coding").unwrap();
    let p = jarvis_memory::persona::Persona {
        persona_md: "## 语气\n简洁直接".into(),
        user_md: "## 用户偏好\n- 函数式".into(),
    };
    let block = render_stable_block(&StablePromptInputs {
        agent: coding,
        persona: Some(&p),
        framework_directives: &["never bypass user confirmation"],
    });
    assert!(block.contains("简洁直接"));
    assert!(block.contains("函数式"));
    assert!(block.contains("never bypass"));
}

#[test]
fn route_emits_growth_event() {
    let db = fresh_db();
    let r = Router::new(db.clone());
    r.route(RouterInput {
        user_input: "OpenWrt DNS 报错",
        session_id_hint: None,
        running_agent_types: &[],
    })
    .unwrap();
    let collector = jarvis_growth::Collector::new(db);
    let n = collector.count_of("route_decision").unwrap();
    assert!(n >= 1, "expected ≥1 route_decision growth event, got {n}");
}

#[test]
fn mention_log_records_unresolved_mentions() {
    let db = fresh_db();
    let r = Router::new(db.clone());
    r.route(RouterInput {
        user_input: "@测试助手 帮我",
        session_id_hint: Some("sess_x"),
        running_agent_types: &[],
    })
    .unwrap();
    let n = jarvis_db::mention_log::count_unresolved(&db).unwrap();
    assert!(n >= 1);
}
