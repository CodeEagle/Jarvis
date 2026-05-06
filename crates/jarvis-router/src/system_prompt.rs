//! System-prompt assembler.
//!
//! Splits the prompt into a stable layer (high cache-hit rate) and a
//! turn layer (per-request data). Section 13.5.

use jarvis_core::agent::AgentDefinition;
use jarvis_memory::persona::Persona;

/// Stable, cache-friendly prefix.
#[derive(Debug, Clone)]
pub struct StablePromptInputs<'a> {
    pub agent: &'a AgentDefinition,
    pub persona: Option<&'a Persona>,
    pub framework_directives: &'a [&'a str],
}

pub fn render_stable_block(inputs: &StablePromptInputs<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# {} ({} {})\n\n",
        inputs.agent.display_name, inputs.agent.avatar_emoji, inputs.agent.r#type
    ));
    if !inputs.agent.tagline.is_empty() {
        out.push_str(&inputs.agent.tagline);
        out.push_str("\n\n");
    }
    if !inputs.agent.capabilities.is_empty() {
        out.push_str("## Capabilities\n\n");
        for c in &inputs.agent.capabilities {
            out.push_str(&format!("- {c}\n"));
        }
        out.push('\n');
    }
    if !inputs.agent.limitations.is_empty() {
        out.push_str("## Limitations\n\n");
        for l in &inputs.agent.limitations {
            out.push_str(&format!("- {l}\n"));
        }
        out.push('\n');
    }
    if !inputs.framework_directives.is_empty() {
        out.push_str("## Framework directives\n\n");
        for d in inputs.framework_directives {
            out.push_str(&format!("- {d}\n"));
        }
        out.push('\n');
    }
    if let Some(p) = inputs.persona {
        let persona_block = persona_section(p);
        if !persona_block.is_empty() {
            out.push_str("## Persona\n\n");
            out.push_str(&persona_block);
            out.push('\n');
        }
    }
    out
}

fn persona_section(p: &Persona) -> String {
    let mut s = String::new();
    if !p.persona_md.trim().is_empty() {
        s.push_str(p.persona_md.trim());
        s.push_str("\n\n");
    }
    if !p.user_md.trim().is_empty() {
        s.push_str(p.user_md.trim());
        s.push('\n');
    }
    s
}
