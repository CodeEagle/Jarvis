import Foundation

/// Thin URLSession wrapper for the local jarvis-api daemon.
///
/// m1 covers: POST /router/input, GET /memory/{scope}. Everything else
/// (SSE, sessions, walkthrough, commands) lands in m2+. The actor
/// boundary keeps URLSession safely isolated; views call `await
/// client.route(...)` from `Task { ... }` blocks.
actor JarvisClient {
    let baseURL: URL
    private let session: URLSession
    private let decoder: JSONDecoder
    private let encoder: JSONEncoder

    init(baseURL: URL) {
        self.baseURL = baseURL
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 15
        config.timeoutIntervalForResource = 60
        self.session = URLSession(configuration: config)
        self.decoder = JSONDecoder()
        self.encoder = JSONEncoder()
    }

    // MARK: Routing

    func route(input: String, sessionId: String? = nil) async throws -> RouteDecision {
        let url = baseURL.appendingPathComponent("router/input")
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try encoder.encode(
            RouteRequest(userInput: input, sessionIdHint: sessionId)
        )
        let (data, resp) = try await session.data(for: req)
        try Self.checkOK(resp, data: data)
        // The API wraps the decision in an envelope; older builds
        // tried to decode RouteDecision directly and produced "the
        // data couldn't be read because it is missing".
        let envelope = try decoder.decode(RouteEnvelope.self, from: data)
        return envelope.decision
    }

    // MARK: Health

    func healthz() async throws -> Bool {
        let url = baseURL.appendingPathComponent("healthz")
        var req = URLRequest(url: url)
        req.timeoutInterval = 2
        let (_, resp) = try await session.data(for: req)
        return (resp as? HTTPURLResponse)?.statusCode == 200
    }

    // MARK: Memory

    func memories(scope: String = "global") async throws -> [MemoryItem] {
        let url = baseURL
            .appendingPathComponent("memory")
            .appendingPathComponent(scope)
        let (data, resp) = try await session.data(from: url)
        try Self.checkOK(resp, data: data)
        // The API may return either `[MemoryItem]` directly or an
        // envelope `{"items": [...]}`. Accept both for resilience —
        // the existing CLI side uses the array shape.
        if let arr = try? decoder.decode([MemoryItem].self, from: data) {
            return arr
        }
        if let env = try? decoder.decode(MemoryListEnvelope.self, from: data) {
            return env.items
        }
        throw JarvisError.parse("memory list shape: expected [MemoryItem] or {items: …}")
    }

    private struct MemoryListEnvelope: Decodable {
        let items: [MemoryItem]
    }

    // MARK: Helpers

    private static func checkOK(_ resp: URLResponse, data: Data) throws {
        guard let http = resp as? HTTPURLResponse else {
            throw JarvisError.transport("non-HTTP response")
        }
        guard (200..<300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw JarvisError.upstream(status: http.statusCode, body: body)
        }
    }
}

enum JarvisError: LocalizedError {
    case transport(String)
    case upstream(status: Int, body: String)
    case parse(String)
    case daemonUnreachable

    var errorDescription: String? {
        switch self {
        case .transport(let s):           return "transport: \(s)"
        case .upstream(let st, let body): return "upstream \(st): \(body.prefix(200))"
        case .parse(let s):               return "parse: \(s)"
        case .daemonUnreachable:          return "jarvis daemon not reachable. Run `jarvis serve` in a terminal."
        }
    }
}
