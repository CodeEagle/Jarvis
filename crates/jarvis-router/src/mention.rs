//! `@mention` parsing. Section 5.3a.

use jarvis_core::agent::AgentDefinition;
use jarvis_core::mention::{AgentMention, MentionMode, MentionParseResult};
use once_cell::sync::Lazy;
use regex::Regex;

static MENTION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"@([A-Za-z0-9_\u{4e00}-\u{9fa5}]+)").expect("regex"));

/// Heuristic: does `clean_input` look like a directional/constraint nudge
/// rather than a fresh task? Used to decide between `single` and `steer`
/// mode when the mentioned agent is currently running. (Section 5.3a.)
pub fn looks_like_steer(clean_input: &str) -> bool {
    let needles = [
        "记得", "注意", "顺便", "对了", "另外", "别忘了", "不要改", "保持", "remember",
        "note that", "btw",
    ];
    needles.iter().any(|n| clean_input.contains(n))
}

pub fn parse_mentions(input: &str, registry: &[AgentDefinition]) -> MentionParseResult {
    let mut mentions = Vec::new();
    let mut clean = String::with_capacity(input.len());
    let mut last_end = 0;
    for cap in MENTION_RE.captures_iter(input) {
        let whole = cap.get(0).expect("match");
        let name = cap.get(1).expect("group 1").as_str();
        clean.push_str(&input[last_end..whole.start()]);
        last_end = whole.end();

        let resolved = registry
            .iter()
            .find(|a| a.matches_mention(name))
            .map(|a| a.r#type.clone());
        mentions.push(AgentMention {
            raw_text: whole.as_str().to_string(),
            unresolved: resolved.is_none(),
            resolved_type: resolved,
        });
    }
    clean.push_str(&input[last_end..]);
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");

    let valid = mentions.iter().filter(|m| !m.unresolved).count();
    let mention_mode = match valid {
        0 => MentionMode::None,
        1 => MentionMode::Single,
        _ => MentionMode::Multi,
    };
    MentionParseResult {
        raw_input: input.to_string(),
        clean_input: clean,
        mentions,
        mention_mode,
    }
}

/// Format the user-facing fallback list: "💻 @代码助手 / 🔧 @运维助手 ..."
pub fn format_mentionable_list(registry: &[AgentDefinition]) -> String {
    registry
        .iter()
        .filter(|a| a.is_mentionable())
        .map(|a| format!("{} @{}", a.avatar_emoji, a.display_name))
        .collect::<Vec<_>>()
        .join(" / ")
}
