//! Trust-score decay and retrieval boost. Section 12.5.

use chrono::{DateTime, Utc};
use jarvis_core::memory::{Memory, MemoryType};

/// Compute the current trust score given the input memory's `confidence`,
/// `half_life_days`, `retrieve_count`, emotion energy, and tier floor.
///
/// - High emotion energy slows decay (Section 12.5).
/// - Each retrieval contributes a small boost, capped at +0.20.
/// - Tier-1 memories never fall below 0.30.
pub fn compute(memory: &Memory, now: DateTime<Utc>) -> f32 {
    let elapsed_days = (now - memory.created_at).num_seconds() as f32 / 86_400.0;

    // Emotion-modulated half-life: energy 10 ≈ 3× the type's half-life.
    let emotion_factor = 1.0 + (memory.emotion_energy / 10.0) * 2.0;
    let effective_half_life = memory.half_life_days as f32 * emotion_factor;

    let decay = if effective_half_life > 0.0 {
        0.5_f32.powf(elapsed_days / effective_half_life)
    } else {
        // Scratch never decays, but it's also not retained long-term;
        // upper layers strip these on session end.
        1.0
    };

    let retrieve_boost = (memory.retrieve_count as f32 * 0.02).min(0.20);
    let raw = memory.confidence * decay + retrieve_boost;

    // Tier-1 floor (Section 12.1a / 30.7).
    let floor = if memory.tier == 1 { 0.30 } else { 0.0 };
    raw.max(floor).clamp(0.0, 1.0)
}

/// Cheap default initial trust score, used the first time we write a memory.
pub fn initial(ty: MemoryType, confidence: f32) -> f32 {
    let _ = ty;
    confidence.clamp(0.0, 1.0)
}
