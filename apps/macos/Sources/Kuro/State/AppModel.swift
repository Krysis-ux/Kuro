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
    /// Settings, shown in the main window with its own sidebar.
    case settings(SettingsPane)

    /// The sidebar row that stays highlighted for this route.
    var sidebarRow: Route {
        switch self {
        case .workspace: .code
        case .project: .projects
        case .model: .models
        default: self
        }
    }

    var isSettings: Bool {
        if case .settings = self { return true }
        return false
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
    /// The last place outside Settings, where "Back to app" goes.
    private(set) var appRoute: Route = .newChat

    private(set) var models: ModelList = .empty
    private(set) var tools: ToolsOverview?
    private(set) var workspaceModes: [WorkspaceModeInfo] = []
    private(set) var conversations: [Conversation] = []
    /// For grouping chats by project in the sidebar and the Move to Project menu.
    private(set) var projects: [Project] = []
    /// The user's own `/` commands.
    private(set) var customCommands: [CustomCommand] = []
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
        if !destination.isSettings { appRoute = destination }
        route = destination
    }

    /// Leave Settings for wherever the user was before.
    func leaveSettings() {
        go(appRoute)
    }

    /// Mark topics stale: shared caches reload here, screens reload on the stamp.
    func invalidate(_ topics: [Topic]) {
        for topic in topics { stamps[topic, default: 0] += 1 }
        Task {
            if topics.contains(.models) { await loadModels() }
            if topics.contains(.tools) { await loadTools() }
            if topics.contains(.conversations) { await loadConversations() }
            if topics.contains(.projects) || topics.contains(.conversations) { await loadProjects() }
            if topics.contains(.workspaces) { await loadWorkspaceModes() }
        }
    }

    func invalidate(_ topics: Topic...) { invalidate(topics) }

    func loadAll() async {
        async let models: Void = loadModels()
        async let tools: Void = loadTools()
        async let conversations: Void = loadConversations()
        async let modes: Void = loadWorkspaceModes()
        async let projects: Void = loadProjects()
        async let commands: Void = loadCustomCommands()
        _ = await (models, tools, conversations, modes, projects, commands)
    }

    func loadCustomCommands() async {
        if let list = try? await client.customCommands() { customCommands = list }
    }

    /// Store the whole list; throws so the editor can show why it failed.
    func saveCustomCommands(_ commands: [CustomCommand]) async throws {
        try await client.saveCustomCommands(commands)
        customCommands = commands
    }

    func loadProjects() async {
        if let list = try? await client.projects() { projects = list }
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

    /// Hide a chat from the sidebar without deleting it; Settings → Archived Chats brings it back.
    func archiveConversation(_ id: String, _ archived: Bool = true) async {
        try? await client.setConversationFlags(id, archived: archived)
        if archived, route == .chat(id) { route = .newChat }
        invalidate(.conversations)
    }

    func pinConversation(_ id: String, _ pinned: Bool) async {
        try? await client.setConversationFlags(id, pinned: pinned)
        invalidate(.conversations)
    }

    func moveConversation(_ id: String, toProject projectId: String?) async {
        try? await client.moveConversation(id, toProject: projectId)
        invalidate(.conversations, .projects)
    }

    func renameConversation(_ id: String, to title: String) async {
        try? await client.renameConversation(id, title: title)
        invalidate(.conversations)
    }
}
