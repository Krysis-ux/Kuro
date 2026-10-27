import Foundation

// Types returned by the Kuro daemon's `/api` routes.
//
// Decoded with `.convertFromSnakeCase`, so `display_name` arrives as
// `displayName` and keys that are already camelCase pass through unchanged.
// Dictionary keys are not converted, which keeps tool arguments verbatim.

// MARK: - Models

public struct FitEstimate: Codable, Hashable, Sendable {
    /// `great`, `fits`, `tight` or `wont_fit`.
    public let verdict: String
    public let label: String
    public let note: String
}

public struct ModelRecord: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let displayName: String
    /// The Hugging Face repository it came from, when it came from one.
    public let hfRepo: String?
    public let quant: String?
    public let family: String?
    public let capabilities: [String]
    public let fileSizeBytes: Int64?
    /// `downloading`, `ready` or `error`.
    public let status: String
    public let error: String?
}

public struct InstalledModel: Codable, Hashable, Sendable, Identifiable {
    public let model: ModelRecord
    public let loaded: Bool
    public let fit: FitEstimate?
    /// `chat`, `embedding`, `reranker`… read off the model name.
    public let kind: String
    /// False for models that cannot hold a conversation (embeddings, rerankers).
    public let chat: Bool

    public var id: String { model.id }
}

/// A model reached through a connected provider, the free pool or a cloud endpoint.
public struct RemoteModel: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let connectorId: String
    public let connectorLabel: String
    /// The provider's slug (`openrouter`, `groq`…), for its logo.
    public let provider: String
    /// Stands for every free model on its provider rather than one model.
    public let pooled: Bool
    public let poolSize: Int
    public let free: Bool
    /// `code`, `reasoning`, `vision`, `fast`, `general`.
    public let specialities: [String]
    public let paramsB: Double?
    /// Why it cannot be picked right now, if it cannot.
    public let unavailable: String?
    public let fixUrl: String?
}

public struct ModelList: Codable, Sendable {
    public let models: [InstalledModel]
    public let remote: [RemoteModel]

    public static let empty = ModelList(models: [], remote: [])

    public var readyLocal: [InstalledModel] { models.filter { $0.model.status == "ready" } }

    /// What the server answers with when no model is chosen.
    public func fallbackId() -> String? {
        readyLocal.first?.model.id ?? remote.first?.id
    }
}

public struct HubModel: Codable, Hashable, Sendable, Identifiable {
    public let repo: String
    public let name: String
    public let owner: String
    public let downloads: Int
    public let likes: Int
    public let lastModified: String?
    public let quants: [String]
    public let paramCount: String?
    public let gated: Bool
    /// Published only as multi-part shards, which cannot be loaded yet.
    public let splitOnly: Bool
    public let fit: FitEstimate?
    public let installed: Bool

    public var id: String { repo }
}

public struct PurposeInfo: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let label: String
    public let blurb: String
}

public struct RecommendedModel: Codable, Hashable, Sendable, Identifiable {
    public let slug: String
    public let displayName: String
    public let repo: String
    public let defaultQuant: String
    public let paramCount: String
    public let capabilities: [String]
    public let purposes: [String]
    public let approxSizeBytes: Int64
    public let blurb: String
    public let installed: Bool
    public let fit: FitEstimate

    public var id: String { slug }
}

public struct RecommendedList: Codable, Sendable {
    public let models: [RecommendedModel]
    public let purposes: [PurposeInfo]
}

public struct LoadedEngine: Codable, Hashable, Sendable, Identifiable {
    public let modelId: String
    public let port: Int
    public let pid: Int
    public let idleSeconds: Int

    public var id: String { modelId }
}

public struct DownloadRecord: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let label: String
    public let totalBytes: Int64?
    public let downloadedBytes: Int64
    /// `queued`, `downloading`, `paused`, `verifying`, `completed`, `failed`, `cancelled`.
    public let status: String
    public let error: String?

    public var isActive: Bool { status == "downloading" || status == "queued" || status == "verifying" }

    public var fraction: Double {
        guard let total = totalBytes, total > 0 else { return 0 }
        return min(1, Double(downloadedBytes) / Double(total))
    }
}

// MARK: - Conversations

public struct Conversation: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let title: String
    public let modelId: String?
    public let pinned: Bool
    public let archived: Bool
    public let updatedAt: String
    public let forkedFromId: String?
}

public struct WebSource: Codable, Hashable, Sendable {
    public let title: String
    public let url: String
}

public struct ToolCallRecord: Codable, Hashable, Sendable {
    public let name: String
    public let ok: Bool
    public let preview: String
    public let arguments: [String: JSONValue]?
}

public struct Message: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let conversationId: String
    /// `user`, `assistant`, `system` or `tool`.
    public let role: String
    public let content: String
    public let reasoningContent: String?
    public let toolCalls: [ToolCallRecord]?
    public let usedWebSearch: Bool
    public let webSources: [WebSource]?
    public let modelId: String?
    public let usagePromptTokens: Int?
    public let usageCompletionTokens: Int?
    public let timingTtftMs: Int?
    public let timingTotalMs: Int?
    public let timingTokensPerSec: Double?
    public let finishReason: String?

    public init(id: String, conversationId: String, role: String, content: String) {
        self.id = id
        self.conversationId = conversationId
        self.role = role
        self.content = content
        reasoningContent = nil
        toolCalls = nil
        usedWebSearch = false
        webSources = nil
        modelId = nil
        usagePromptTokens = nil
        usageCompletionTokens = nil
        timingTtftMs = nil
        timingTotalMs = nil
        timingTokensPerSec = nil
        finishReason = nil
    }
}

// MARK: - Projects

public struct Project: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let description: String
    public let instructions: String
    public let modelId: String?
    public let updatedAt: String
    public let conversationCount: Int
}

// MARK: - Coding workspaces

public enum WorkspaceMode: String, Codable, CaseIterable, Sendable {
    case ask, plan, agent, bypass

    /// Agent and Bypass can run commands; Ask and Plan cannot.
    public var canRun: Bool { self == .agent || self == .bypass }
}

public struct Workspace: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let rootPath: String
    public let modelId: String?
    public let mode: WorkspaceMode
    /// Whether the folder is still on disk.
    public let rootExists: Bool
    public let conversationCount: Int
}

public struct WorkspaceModeInfo: Codable, Hashable, Sendable, Identifiable {
    public let id: WorkspaceMode
    public let label: String
    public let blurb: String
}

public struct WorkspaceList: Codable, Sendable {
    public let workspaces: [Workspace]
    public let modes: [WorkspaceModeInfo]
}

public struct WorkspaceDetail: Codable, Sendable {
    public let workspace: Workspace
    public let conversations: [Conversation]
}

/// A command a workspace left running. `url` appears once the process prints
/// the address it is serving on.
public struct RunningProcess: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let command: String
    public let startedAt: String
    public let url: String?
    public let running: Bool
    public let exitCode: Int?
}

/// One file change a model made.
public struct WorkspaceChange: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let path: String
    /// `edit` or `write`.
    public let kind: String
    public let undone: Bool
    public let undoable: Bool
    /// The model created this file, so undoing removes it.
    public let created: Bool
    public let beforeLines: Int?
    public let afterLines: Int?
}

/// Both sides of one change. `after` is nil when the file was too large to snapshot.
public struct ChangeDetail: Codable, Sendable {
    public let path: String
    public let before: String?
    public let after: String?
}

// MARK: - Tools, skills and memory

public struct BuiltinTool: Codable, Hashable, Sendable, Identifiable {
    public let name: String
    public let description: String
    public let group: String

    public var id: String { name }
}

public struct SearchProviderOption: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let note: String
    public let needsApiKey: Bool
    public let needsBaseUrl: Bool
    public let credentialsUrl: String?
}

public struct Skill: Codable, Hashable, Sendable, Identifiable {
    public let slug: String
    public let name: String
    public let blurb: String
    /// `language`, `coding`, `practice`, `design`, `writing`, or a custom category.
    public let category: String
    public let instructions: String

    public var id: String { slug }
}

/// Always on in a coding workspace; has no switch.
public struct EssentialSkill: Codable, Hashable, Sendable, Identifiable {
    public let slug: String
    public let name: String
    public let blurb: String

    public var id: String { slug }
}

/// A skill the user uploaded or imported.
public struct CustomSkill: Codable, Hashable, Sendable, Identifiable {
    public let slug: String
    public let name: String
    /// `upload`, or the repository it came from.
    public let source: String

    public var id: String { slug }
}

public struct SkillImport: Codable, Hashable, Sendable {
    public let repo: String
    public let added: [String]
    /// Names that clashed with a built-in.
    public let skipped: [String]
    /// Skills that expect `scripts/` which were not imported.
    public let needsScripts: [String]
}

public struct SkillBudget: Codable, Hashable, Sendable {
    public let effort: String
    public let tokens: Int
}

/// Per-surface preferences. Chat and coding are configured separately.
public struct SurfaceSettings: Codable, Hashable, Sendable {
    public let autoOrchestrate: Bool
    public let defaultEffort: Effort
    /// Coding only: the mode a new workspace starts in.
    public let defaultMode: WorkspaceMode?
}

public struct ToolsOverview: Codable, Sendable {
    public struct Skills: Codable, Sendable {
        public struct Budgets: Codable, Sendable {
            public let chat: [SkillBudget]
            public let code: [SkillBudget]
        }

        public let catalogue: [Skill]
        public let enabled: [String]
        public let budgets: Budgets?
        public let essentials: [EssentialSkill]
        public let custom: [CustomSkill]?
    }

    public struct Surfaces: Codable, Sendable {
        public let chat: SurfaceSettings
        public let code: SurfaceSettings
    }

    public struct Memory: Codable, Sendable {
        public let preload: Bool
        public let count: Int
    }

    public struct Search: Codable, Sendable {
        public let provider: String
        public let baseUrl: String?
        /// Whether a key is stored. The key itself is never sent.
        public let hasApiKey: Bool
        public let needsApiKey: Bool
        public let needsBaseUrl: Bool
        public let providers: [SearchProviderOption]
    }

    public let builtins: [BuiltinTool]
    public let skills: Skills
    public let surfaces: Surfaces
    public let memory: Memory
    public let search: Search
}

public struct SkillImportResult: Codable, Sendable {
    public let imported: SkillImport
}

public struct SearchResult: Codable, Hashable, Sendable {
    public let title: String
    public let url: String
}

public struct SearchTest: Codable, Sendable {
    public let ok: Bool
    public let results: [SearchResult]?
    public let error: String?
}

public struct MemoryRecord: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let content: String
    public let tags: [String]
}

// MARK: - MCP

public struct ExposedTool: Codable, Hashable, Sendable, Identifiable {
    public let name: String
    public let description: String

    public var id: String { name }
}

public struct McpServer: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    /// `stdio` or `http`.
    public let transport: String
    public let command: String?
    public let args: [String]
    public let url: String?
    public let enabled: Bool
    /// `connected`, `disconnected` or `error`.
    public let status: String
    public let lastError: String?
    public let toolCount: Int?
    public let tools: [ExposedTool]
    /// Whether a bearer token is stored. Never the token.
    public let hasAuth: Bool

    public var address: String {
        url ?? ([command ?? ""] + args).filter { !$0.isEmpty }.joined(separator: " ")
    }
}

public struct McpRegistryEntry: Codable, Hashable, Sendable, Identifiable {
    public let slug: String
    public let name: String
    public let blurb: String
    public let detail: String
    public let transport: String
    public let url: String?
    public let command: String?
    public let args: [String]
    /// `none`, `api_key` or `local_runtime`.
    public let requirement: String
    public let credentialsUrl: String?
    public let homepage: String
    public let installed: Bool

    public var id: String { slug }
}

public struct ConnectionResult: Codable, Sendable {
    public let ok: Bool
    public let toolCount: Int?
    public let error: String?
}

public struct AddedServer: Codable, Sendable {
    public let connection: ConnectionResult
}

// MARK: - Providers and cloud endpoints

public struct ProviderPreset: Codable, Hashable, Sendable, Identifiable {
    public let slug: String
    public let name: String
    public let blurb: String
    /// `aggregator`, `first_party`, `rented_gpu` or `custom`.
    public let kind: String
    public let credentialsUrl: String?
    public let keyHint: String?
    public let needsUrl: Bool
    /// `provider` (someone else's model) or `cloud` (your model, rented hardware).
    public let surface: String

    public var id: String { slug }
}

public struct Provider: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let provider: String
    public let label: String
    public let baseUrl: String
    /// `untested`, `ok` or `error`.
    public let status: String
    public let lastTestedAt: String?
    public let lastError: String?
    public let enabled: Bool
    public let models: [String]
    public let hasKey: Bool
    public let surface: String
}

public struct ProviderList: Codable, Sendable {
    public let providers: [Provider]
    public let presets: [ProviderPreset]
}

// MARK: - Free-tier pool

public struct FreeLimit: Codable, Hashable, Sendable {
    public let tokensPerMonth: Int?
}

public struct FreeProvider: Codable, Hashable, Sendable, Identifiable {
    public struct Privacy: Codable, Hashable, Sendable {
        public let logged: Bool
        public let trains: Bool
    }

    public let slug: String
    public let name: String
    public let baseUrl: String
    public let credentialsUrl: String
    public let allowance: String
    public let keyHint: String?
    public let hasKey: Bool
    /// `rate_limited`, `rejected` or `model_gone` while being skipped.
    public let trouble: String?
    /// A shared endpoint: no account, no key, rate limited by address.
    public let keyless: Bool
    public let privacy: Privacy
    /// A trial past its date; its key is no longer used.
    public let expired: Bool
    public let limit: FreeLimit?

    public var id: String { slug }
}

public struct FreeFlavour: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let blurb: String
    public let available: Bool
}

public struct FreeOverview: Codable, Sendable {
    public let providers: [FreeProvider]
    public let flavours: [FreeFlavour]
    public let keyCount: Int
    public let availableCount: Int
    public let allowKeyless: Bool
}

public struct FreeProviderUsage: Codable, Hashable, Sendable, Identifiable {
    public let providerSlug: String
    public let name: String
    public let turns: Int
    public let totalTokens: Int
    public let limit: FreeLimit?

    public var id: String { providerSlug }
}

public struct FreeWindow: Codable, Sendable {
    public let providers: [FreeProviderUsage]
    public let turns: Int
    public let unreportedTurns: Int
    public let totalTokens: Int
}

public struct FreeUsage: Codable, Sendable {
    public struct Averages: Codable, Sendable {
        public let tokensPerTurn: Double?
        public let tokensPerDayThisMonth: Double?
    }

    public let day: FreeWindow
    public let month: FreeWindow
    public let averages: Averages
}

public struct KeyTest: Codable, Sendable {
    public let ok: Bool
    public let error: String?
}

// MARK: - System

public struct HardwareInfo: Codable, Sendable {
    public struct Recommended: Codable, Sendable {
        public let contextSize: Int
        public let gpuLayers: Int
        public let threads: Int
    }

    public let chip: String?
    public let totalMemoryBytes: Int64
    public let physicalCores: Int
    public let logicalCores: Int
    public let gpuAvailable: Bool
    public let gpuBackend: String
    public let recommended: Recommended
}

public struct HardwareReport: Codable, Sendable {
    public let hardware: HardwareInfo
}

public struct ServerStatus: Codable, Sendable {
    public let version: String
    public let address: String
    public let uptimeSeconds: Int
    public let dataDirectory: String
    public let loadedModels: [LoadedEngine]
}

/// How hard to think. `ultra` is offered only in a coding workspace.
public enum Effort: String, Codable, CaseIterable, Sendable {
    case low, balanced, high, max, ultra

    public var label: String {
        switch self {
        case .low: "Instant"
        case .balanced: "Balanced"
        case .high: "Thinking"
        case .max: "Extended"
        case .ultra: "Ultracode"
        }
    }

    public var detail: String {
        switch self {
        case .low: "Answers straight away, briefly. Best for quick questions."
        case .balanced: "The default. Enough room to use a tool or two."
        case .high: "Works through the problem, reads more, and checks itself."
        case .max: "The most thinking, the most tool use, the longest answers."
        case .ultra:
            "Every coding skill, every tool, and as long as it takes. For a change that spans several files and has to be built and tested."
        }
    }

    /// Whether this level makes the model reason before answering.
    public var thinks: Bool { self == .high || self == .max || self == .ultra }

    public static func options(coding: Bool) -> [Effort] {
        coding ? allCases : allCases.filter { $0 != .ultra }
    }
}
