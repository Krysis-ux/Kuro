import SwiftUI
import KuroKit

/// Where the main window is.
enum Route: Hashable {
    case newChat
    case chat(String)
    case code
    case workspace(String)
    case projects
    case project(String)
    case models
    /// One Hugging Face repository, `owner/name`.
    case model(String)
    case free
    case tools
    case providers
    case cloud

    /// The sidebar row that stays highlighted for this route.
    var sidebarRow: Route {
        switch self {
        case .workspace: .code
        case .project: .projects
        case .model: .models
        default: self
        }
    }
}

/// Data that more than one screen reads. Screens reload it with `invalidate`
/// and watch `stamp(_:)` to reload their own data.
enum Topic: Hashable {
    case conversations, models, tools, projects, workspaces, free, providers, mcp
    case workspace(String)
}

@MainActor @Observable
final class AppModel {
    let server = ServerController()
    let prefs = Preferences()
    let turns = TurnStore()

    var client: KuroClient { server.client }

    var route: Route = .newChat
    /// Where "Back" returns to, when a screen was opened from the composer.
    var returnRoute: Route?

    private(set) var models: ModelList = .empty
    private(set) var tools: ToolsOverview?
    private(set) var workspaceModes: [WorkspaceModeInfo] = []
    private(set) var conversations: [Conversation] = []
    var conversationQuery = ""
    /// Messages by conversation.
    private(set) var messages: [String: [Message]] = [:]

    private(set) var stamps: [Topic: Int] = [:]

    init() {
        turns.app = self
        #if DEBUG
        DebugDriver.install(self)
        #endif
    }

    func stamp(_ topic: Topic) -> Int { stamps[topic] ?? 0 }

    /// Reload what a finished turn may have changed. A provider that refused is
    /// greyed out in the model list, and a coding turn may have edited files.
    func turnFinished(_ conversationId: String, workspace: String?) async {
        await loadMessages(conversationId)
        var topics: [Topic] = [.conversations, .models]
        if let workspace { topics.append(.workspace(workspace)) }
        invalidate(topics)
    }

    func go(_ destination: Route, returning: Bool = false) {
        returnRoute = returning ? route : nil
        route = destination
    }

    /// Mark topics stale: shared caches reload here, screens reload on the stamp.
    func invalidate(_ topics: [Topic]) {
        for topic in topics { stamps[topic, default: 0] += 1 }
        Task {
            if topics.contains(.models) { await loadModels() }
            if topics.contains(.tools) { await loadTools() }
            if topics.contains(.conversations) { await loadConversations() }
            if topics.contains(.workspaces) { await loadWorkspaceModes() }
        }
    }

    func invalidate(_ topics: Topic...) { invalidate(topics) }

    func loadAll() async {
        async let models: Void = loadModels()
        async let tools: Void = loadTools()
        async let conversations: Void = loadConversations()
        async let modes: Void = loadWorkspaceModes()
        _ = await (models, tools, conversations, modes)
    }

    func loadModels() async {
        if let list = try? await client.models() { models = list }
    }

    func loadTools() async {
        guard let overview = try? await client.tools() else { return }
        tools = overview
        prefs.seedEfforts(chat: overview.surfaces.chat.defaultEffort, code: overview.surfaces.code.defaultEffort)
    }

    func loadConversations() async {
        if let list = try? await client.conversations(matching: conversationQuery) { conversations = list }
    }

    func loadWorkspaceModes() async {
        if let list = try? await client.workspaces() { workspaceModes = list.modes }
    }

    func loadMessages(_ conversationId: String) async {
        if let list = try? await client.messages(conversationId) { messages[conversationId] = list }
    }

    // MARK: Conversation actions shared by Chat and Code

    /// Branch a conversation at a message. The original is left as it was.
    func fork(_ conversationId: String, at messageId: String) async -> Conversation? {
        do {
            let branch = try await client.fork(conversationId, upTo: messageId)
            invalidate(.conversations)
            return branch
        } catch {
            turns.fail(conversationId, error.localizedDescription)
            return nil
        }
    }

    func deleteConversation(_ id: String) async {
        try? await client.deleteConversation(id)
        if route == .chat(id) { route = .newChat }
        invalidate(.conversations)
    }

    func renameConversation(_ id: String, to title: String) async {
        try? await client.renameConversation(id, title: title)
        invalidate(.conversations)
    }
}
