//! Rule-layer intent hints (Section 5.4).

use jarvis_core::intent::IntentHint;
use once_cell::sync::Lazy;
use regex::Regex;

struct Rule {
    pattern: Regex,
    hint: &'static str,
    weight: f32,
    matched_label: &'static str,
}

static RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    let mk = |raw: &str, hint: &'static str, weight: f32, matched: &'static str| Rule {
        pattern: Regex::new(raw).expect("valid regex"),
        hint,
        weight,
        matched_label: matched,
    };
    vec![
        // ── coding ─────────────────────────────────────────────────────
        mk(
            r"(?i)报错|error|exception|failed|denied|stack trace",
            "coding.debug",
            0.40,
            "coding.debug keywords",
        ),
        mk(
            r"(?i)重构|refactor|rewrite",
            "coding.refactor",
            0.35,
            "coding.refactor keywords",
        ),
        mk(
            r"(?i)实现|实现一下|帮我写|生成代码|写一段",
            "coding.generate",
            0.35,
            "coding.generate keywords",
        ),
        // ── devops ─────────────────────────────────────────────────────
        mk(
            r"(?i)openwrt|immortalwrt|homeproxy|sing-box|dnsmasq|端口转发|防火墙",
            "devops.networking",
            0.40,
            "devops.networking keywords",
        ),
        mk(
            r"(?i)docker|compose|镜像|container",
            "devops.docker",
            0.35,
            "devops.docker keywords",
        ),
        // ── creative — note: rule matches creative.icon (not creative.design)
        // to align with L2 intent definitions in Section 5.3 / 30.1.1.
        mk(
            r"(?i)图标|icon|prompt|UI|界面|生成图",
            "creative.icon",
            0.35,
            "creative.icon keywords",
        ),
        // ── memory ─────────────────────────────────────────────────────
        mk(
            r"(?i)记住|以后|从现在开始|忘记",
            "memory.update",
            0.80,
            "memory.update keywords",
        ),
        // ── research ───────────────────────────────────────────────────
        mk(
            r"(?i)搜一下|搜索|查一下|对比|比较一下",
            "research.web_search",
            0.30,
            "research keywords",
        ),
    ]
});

/// Apply the rule layer. Returns one hint per rule that matched. Hints are
/// advisory: they're handed to the LLM judge as weighted suggestions.
pub fn apply_rules(input: &str) -> Vec<IntentHint> {
    let mut hits = Vec::new();
    for rule in RULES.iter() {
        if rule.pattern.is_match(input) {
            hits.push(IntentHint {
                hint: rule.hint.to_string(),
                weight: rule.weight,
                matched: rule.matched_label.to_string(),
            });
        }
    }
    hits
}

/// Pick the dominant intent suggestion from a hint list.
pub fn dominant_hint(hints: &[IntentHint]) -> Option<&IntentHint> {
    hints.iter().max_by(|a, b| {
        a.weight
            .partial_cmp(&b.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}
