import Foundation

/// Mirror of `jarvis_core::route::RouteDecision`. Field names use
/// snake_case on the wire; we map to camelCase via CodingKeys so
/// SwiftLint stays happy.
struct RouteDecision: Codable, Equatable {
    let traceId: String
    let taskId: String
    let primaryIntent: String
    let secondaryIntents: [String]
    let domain: String
    let topic: String
    let agentType: String
    let confidence: Double
    let clarificationNeeded: Bool
    let mentionOverride: Bool
    let fallbackUsed: Bool
    let routerNotes: String

    private enum CodingKeys: String, CodingKey {
        case traceId             = "trace_id"
        case taskId              = "task_id"
        case primaryIntent       = "primary_intent"
        case secondaryIntents    = "secondary_intents"
        case domain
        case topic
        case agentType           = "agent_type"
        case confidence
        case clarificationNeeded = "clarification_needed"
        case mentionOverride     = "mention_override"
        case fallbackUsed        = "fallback_used"
        case routerNotes         = "router_notes"
    }
}

/// Mirror of `jarvis_core::memory::MemoryItem`. We only decode the
/// fields m1 actually displays — extra fields the API returns are
/// ignored thanks to default Codable strictness on missing keys.
struct MemoryItem: Codable, Identifiable, Equatable {
    let id: String
    let content: String
    let memoryType: String?
    let trustScore: Double?
    let tier: String?
    let createdAt: String?

    private enum CodingKeys: String, CodingKey {
        case id, content, tier
        case memoryType = "memory_type"
        case trustScore = "trust_score"
        case createdAt  = "created_at"
    }
}

// MARK: - Request envelopes

struct RouteRequest: Encodable {
    let userInput: String
    let sessionIdHint: String?

    private enum CodingKeys: String, CodingKey {
        case userInput     = "user_input"
        case sessionIdHint = "session_id_hint"
    }
}
