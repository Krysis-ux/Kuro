import Foundation

/// An error carrying the server's own message, so the interface never invents wording.
public struct APIError: LocalizedError, Sendable {
    public let message: String
    public let status: Int

    public var errorDescription: String? { message }
}

/// HTTP client for the Kuro daemon. Mirrors the routes in `kuro-server`.
public struct KuroClient: Sendable {
    public let base: URL

    public init(base: URL) {
        self.base = base
    }

    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()

    // MARK: Transport

    /// A JSON body. `nil` values are sent as `null`, which the server reads as "clear".
    public typealias Body = [String: Any?]

    func urlRequest(_ method: String, _ path: String, query: [String: String] = [:], body: Body? = nil) throws -> URLRequest {
        // Paths arrive already escaped (see `segment`), so they are joined as text
        // rather than through `appendingPathComponent`, which would escape them twice.
        var components = URLComponents(string: base.absoluteString + "/" + path)!
        if !query.isEmpty {
            components.queryItems = query.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) }
        }
        var request = URLRequest(url: components.url!)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let body {
            request.httpBody = try JSONSerialization.data(withJSONObject: body.mapValues { $0 ?? NSNull() })
        } else if method != "GET" && method != "DELETE" {
            request.httpBody = Data("{}".utf8)
        }
        return request
    }

    @discardableResult
    func raw(_ method: String, _ path: String, query: [String: String] = [:], body: Body? = nil) async throws -> Data {
        let request = try urlRequest(method, path, query: query, body: body)
        let (data, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else { throw Self.error(from: data, status: status) }
        return data
    }

    func call<T: Decodable>(_ method: String, _ path: String, query: [String: String] = [:], body: Body? = nil) async throws -> T {
        let data = try await raw(method, path, query: query, body: body)
        return try Self.decoder.decode(T.self, from: data)
    }

    func get<T: Decodable>(_ path: String, query: [String: String] = [:]) async throws -> T {
        try await call("GET", path, query: query)
    }

    /// Escape one path segment. Model ids can contain `/` and `:`.
    static func segment(_ value: String) -> String {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return value.addingPercentEncoding(withAllowedCharacters: allowed) ?? value
    }

    static func error(from data: Data, status: Int) -> APIError {
        if let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let error = object["error"] as? [String: Any],
           let message = error["message"] as? String {
            return APIError(message: message, status: status)
        }
        let text = String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        return APIError(message: text.isEmpty ? "Request failed (\(status))" : text, status: status)
    }

    func list<T: Decodable>(_ path: String, key: String, query: [String: String] = [:]) async throws -> T {
        let data = try await raw("GET", path, query: query)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
        let field = try JSONSerialization.data(withJSONObject: object[key] ?? [])
        return try Self.decoder.decode(T.self, from: field)
    }

    // MARK: System

    public func isHealthy() async -> Bool {
        var request = URLRequest(url: base.appendingPathComponent("api/health"))
        request.timeoutInterval = 2
        request.cachePolicy = .reloadIgnoringLocalCacheData
        guard let (_, response) = try? await URLSession.shared.data(for: request) else { return false }
        return (response as? HTTPURLResponse)?.statusCode == 200
    }

    public func status() async throws -> ServerStatus { try await get("api/status") }
    public func hardware() async throws -> HardwareInfo { (try await get("api/hardware") as HardwareReport).hardware }
    public func shutdown() async throws { try await raw("POST", "api/shutdown") }
    public func restart() async throws { try await raw("POST", "api/restart") }

    // MARK: Models and downloads

    public func models() async throws -> ModelList { try await get("api/models") }
    public func recommended() async throws -> RecommendedList { try await get("api/models/recommended") }

    public func searchHub(_ query: String) async throws -> [HubModel] {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        return try await list("api/models/search", key: "models", query: trimmed.isEmpty ? [:] : ["q": trimmed])
    }

    /// The model card, papers and every weights file of one Hub repository.
    public func modelDetails(_ repo: String) async throws -> HubModelCard {
        try await get("api/models/details", query: ["repo": repo])
    }

    public func pull(_ reference: String) async throws { try await raw("POST", "api/models/pull", body: ["model": reference]) }
    public func deleteModel(_ id: String) async throws { try await raw("DELETE", "api/models/\(Self.segment(id))") }
    public func loadModel(_ id: String) async throws { try await raw("POST", "api/models/\(Self.segment(id))/load") }
    public func unloadModel(_ id: String) async throws { try await raw("POST", "api/models/\(Self.segment(id))/unload") }
    public func downloads() async throws -> [DownloadRecord] { try await list("api/downloads", key: "downloads") }
    public func cancelDownload(_ id: String) async throws { try await raw("POST", "api/downloads/\(id)/cancel") }

    // MARK: Conversations

    public func conversations(matching query: String = "") async throws -> [Conversation] {
        try await list("api/conversations", key: "conversations", query: query.isEmpty ? [:] : ["q": query])
    }

    public func createConversation(model: String?) async throws -> Conversation {
        try await call("POST", "api/conversations", body: ["model_id": model])
    }

    public func renameConversation(_ id: String, title: String) async throws {
        try await raw("PATCH", "api/conversations/\(id)", body: ["title": title])
    }

    public func deleteConversation(_ id: String) async throws { try await raw("DELETE", "api/conversations/\(id)") }

    public func messages(_ conversationId: String) async throws -> [Message] {
        try await list("api/conversations/\(conversationId)/messages", key: "messages")
    }

    /// Branch a conversation into a new one, up to and including a message.
    public func fork(_ conversationId: String, upTo messageId: String) async throws -> Conversation {
        try await call("POST", "api/conversations/\(conversationId)/fork", body: ["up_to_message_id": messageId])
    }

    /// `nil` moves the conversation out of any project.
    public func moveConversation(_ id: String, toProject projectId: String?) async throws {
        try await raw("POST", "api/conversations/\(id)/project", body: ["projectId": projectId])
    }

    // MARK: Projects

    public func projects() async throws -> [Project] { try await list("api/projects", key: "projects") }

    public func project(_ id: String) async throws -> (Project, [Conversation]) {
        struct Detail: Decodable { let project: Project; let conversations: [Conversation] }
        let detail: Detail = try await get("api/projects/\(id)")
        return (detail.project, detail.conversations)
    }

    public func createProject(name: String) async throws -> Project {
        struct Created: Decodable { let project: Project }
        return (try await call("POST", "api/projects", body: ["name": name]) as Created).project
    }

    public func updateProject(_ id: String, _ patch: Body) async throws { try await raw("PATCH", "api/projects/\(id)", body: patch) }
    public func deleteProject(_ id: String) async throws { try await raw("DELETE", "api/projects/\(id)") }

    // MARK: Workspaces

    public func workspaces() async throws -> WorkspaceList { try await get("api/workspaces") }

    public func createWorkspace(name: String, root: String) async throws -> Workspace {
        try await call("POST", "api/workspaces", body: ["name": name, "root_path": root])
    }

    public func workspace(_ id: String) async throws -> WorkspaceDetail { try await get("api/workspaces/\(id)") }
    public func updateWorkspace(_ id: String, _ patch: Body) async throws { try await raw("PATCH", "api/workspaces/\(id)", body: patch) }
    public func deleteWorkspace(_ id: String) async throws { try await raw("DELETE", "api/workspaces/\(id)") }
    public func tree(_ id: String) async throws -> [String] { try await list("api/workspaces/\(id)/tree", key: "entries") }

    public func file(_ id: String, path: String) async throws -> String {
        struct File: Decodable { let content: String }
        return (try await get("api/workspaces/\(id)/file", query: ["path": path]) as File).content
    }

    public func changes(_ id: String) async throws -> [WorkspaceChange] { try await list("api/workspaces/\(id)/changes", key: "changes") }
    public func change(_ id: String, _ changeId: String) async throws -> ChangeDetail { try await get("api/workspaces/\(id)/changes/\(changeId)") }
    public func undo(_ id: String, _ changeId: String) async throws { try await raw("POST", "api/workspaces/\(id)/changes/\(changeId)/undo") }
    public func newWorkspaceConversation(_ id: String) async throws -> Conversation { try await call("POST", "api/workspaces/\(id)/conversations") }

    public func processes(_ id: String) async throws -> [RunningProcess] { try await list("api/workspaces/\(id)/processes", key: "processes") }

    public func startProcess(_ id: String, command: String) async throws -> RunningProcess {
        struct Started: Decodable { let process: RunningProcess }
        return (try await call("POST", "api/workspaces/\(id)/processes", body: ["command": command]) as Started).process
    }

    public func processLog(_ id: String, _ processId: String) async throws -> [String] {
        try await list("api/workspaces/\(id)/processes/\(processId)/log", key: "lines")
    }

    public func stopProcess(_ id: String, _ processId: String) async throws { try await raw("POST", "api/workspaces/\(id)/processes/\(processId)/stop") }
    public func forgetProcess(_ id: String, _ processId: String) async throws { try await raw("DELETE", "api/workspaces/\(id)/processes/\(processId)") }
    public func clearProcesses(_ id: String) async throws { try await raw("DELETE", "api/workspaces/\(id)/processes") }

    // MARK: Free pool

    public func free() async throws -> FreeOverview { try await get("api/free") }
    public func freeUsage() async throws -> FreeUsage { try await get("api/free/usage") }
    public func setFreeKey(_ slug: String, key: String) async throws { try await raw("POST", "api/free/\(Self.segment(slug))/key", body: ["apiKey": key]) }
    public func removeFreeKey(_ slug: String) async throws { try await raw("DELETE", "api/free/\(Self.segment(slug))/key") }
    public func testFreeKey(_ slug: String) async throws -> KeyTest { try await call("POST", "api/free/\(Self.segment(slug))/test") }
    public func setKeyless(_ enabled: Bool) async throws { try await raw("POST", "api/free/keyless", body: ["enabled": enabled]) }

    public func setFreeLimit(_ slug: String, tokensPerMonth: Int?) async throws {
        if let tokensPerMonth {
            try await raw("PUT", "api/free/\(Self.segment(slug))/allowance", body: ["tokensPerMonth": tokensPerMonth])
        } else {
            try await raw("DELETE", "api/free/\(Self.segment(slug))/allowance")
        }
    }

    // MARK: Settings

    public func settings() async throws -> [String: JSONValue] { try await get("api/settings") }
    public func patchSettings(_ patch: Body) async throws { try await raw("PATCH", "api/settings", body: patch) }

    // MARK: Tools and skills

    public func tools() async throws -> ToolsOverview { try await get("api/tools") }
    public func setMemoryPreload(_ on: Bool) async throws { try await raw("POST", "api/tools/defaults", body: ["memoryPreload": on]) }
    /// The whole enabled set is sent, so concurrent toggles cannot disagree.
    public func setSkills(_ enabled: [String]) async throws { try await raw("POST", "api/tools/skills", body: ["enabled": enabled]) }
    public func uploadSkill(filename: String, content: String) async throws {
        try await raw("POST", "api/tools/skills/upload", body: ["filename": filename, "content": content])
    }
    public func importSkills(from url: String) async throws -> SkillImport {
        (try await call("POST", "api/tools/skills/import", body: ["url": url]) as SkillImportResult).imported
    }
    public func deleteSkill(_ slug: String) async throws { try await raw("DELETE", "api/tools/skills/\(Self.segment(slug))") }
    public func configureSearch(_ patch: Body) async throws { try await raw("POST", "api/tools/search", body: patch) }
    public func testSearch() async throws -> SearchTest { try await call("POST", "api/tools/search/test") }

    public func memories() async throws -> [MemoryRecord] { try await list("api/memories", key: "memories") }
    public func deleteMemory(_ id: String) async throws { try await raw("DELETE", "api/memories/\(id)") }

    // MARK: MCP

    /// `connect` dials every enabled server first; slower, but statuses are live.
    public func mcpServers(connect: Bool) async throws -> [McpServer] {
        try await list("api/mcp/servers", key: "servers", query: connect ? ["connect": "true"] : [:])
    }

    public func mcpRegistry() async throws -> [McpRegistryEntry] { try await list("api/mcp/registry", key: "entries") }
    public func addMcpServer(_ body: Body) async throws -> ConnectionResult { (try await call("POST", "api/mcp/servers", body: body) as AddedServer).connection }
    public func refreshMcpServer(_ id: String) async throws { try await raw("POST", "api/mcp/servers/\(id)/refresh") }
    public func setMcpEnabled(_ id: String, _ on: Bool) async throws { try await raw("POST", "api/mcp/servers/\(id)/enabled", body: ["enabled": on]) }
    public func setMcpToken(_ id: String, _ token: String) async throws { try await raw("POST", "api/mcp/servers/\(id)/auth", body: ["authToken": token]) }
    public func deleteMcpServer(_ id: String) async throws { try await raw("DELETE", "api/mcp/servers/\(id)") }

    // MARK: Providers and cloud

    public func providers() async throws -> ProviderList { try await get("api/providers") }

    public func addProvider(_ body: Body) async throws -> Provider {
        struct Added: Decodable { let provider: Provider }
        return (try await call("POST", "api/providers", body: body) as Added).provider
    }

    public func testProvider(_ id: String) async throws { try await raw("POST", "api/providers/\(id)/test") }
    public func replaceProviderKey(_ id: String, _ key: String) async throws { try await raw("POST", "api/providers/\(id)/key", body: ["apiKey": key]) }
    public func setProviderEnabled(_ id: String, _ on: Bool) async throws { try await raw("POST", "api/providers/\(id)/enabled", body: ["enabled": on]) }
    public func deleteProvider(_ id: String) async throws { try await raw("DELETE", "api/providers/\(id)") }
}

