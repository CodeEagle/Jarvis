import Foundation

/// Wire shape of `POST /router/input` — the API wraps a RouteDecision
/// in a `{"decision": ..., "diagnostics": {...}}` envelope. Earlier
/// builds tried to decode the inner shape directly and failed with
/// "the data couldn't be read because it is missing".
struct RouteEnvelope: Decodable {
    let decision: RouteDecision
    let diagnostics: RouteDiagnostics?
}

struct RouteDiagnostics: Decodable {
    let rawEventSeq: Int?
    let hadExplicitReference: Bool?
    let mentionWarning: String?

    private enum CodingKeys: String, CodingKey {
        case rawEventSeq           = "raw_event_seq"
        case hadExplicitReference  = "had_explicit_reference"
        case mentionWarning        = "mention_warning"
    }
}

/// Mirror of `jarvis_core::route::RouteDecision`.
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

/// Mirror of `jarvis_core::memory::Memory`. The wire JSON uses `type`
/// (Rust `r#type` field), not `memory_type` — earlier builds had
/// the wrong CodingKey. We decode liberally: every field is optional
/// or has a default so a partial response still works.
struct MemoryItem: Codable, Identifiable, Equatable {
    let id: String
    let content: String
    let type: String?
    let scope: String?
    let trustScore: Double?
    let confidence: Double?
    let tier: Int?
    let status: String?
    let createdAt: String?
    let updatedAt: String?
    let entities: [String]?

    private enum CodingKeys: String, CodingKey {
        case id, content, type, scope, tier, status, entities
        case trustScore = "trust_score"
        case confidence
        case createdAt  = "created_at"
        case updatedAt  = "updated_at"
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

// MARK: - Provider config (mirrors jarvis-llm's LlmConfig as exposed
// by `jarvis-cli model list --json`).

struct ProviderInfo: Decodable, Identifiable, Equatable {
    var id: String { name }
    let name: String
    let apiKeyEnv: String?
    let oauthBinary: String?
    let oauthKind: String?
    let baseUrl: String?
    let authed: Bool

    private enum CodingKeys: String, CodingKey {
        case name
        case apiKeyEnv     = "api_key_env"
        case oauthBinary   = "oauth_binary"
        case oauthKind     = "oauth_kind"
        case baseUrl       = "base_url"
        case authed
    }
}

struct ModelListPayload: Decodable, Equatable {
    let defaultModel: String?
    let configPath: String
    let providers: [ProviderInfo]

    private enum CodingKeys: String, CodingKey {
        case defaultModel = "default_model"
        case configPath   = "config_path"
        case providers
    }
}

