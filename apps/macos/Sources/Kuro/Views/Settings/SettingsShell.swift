import SwiftUI
import KuroKit

/// One page of Settings. Settings live in the main window, with their own sidebar.
enum SettingsPane: String, CaseIterable, Hashable, Identifiable {
    case general, chat, coding, archived
    case providers, usage
    case personalization, subagents, skills, commands, automations, mcp, search, tools
    case storage, engine, server

    var id: String { rawValue }

    enum Group: String, CaseIterable {
        case basics = "Basics"
        case models = "Models"
        case agent = "Agent"
        case system = "System"

        var panes: [SettingsPane] { SettingsPane.allCases.filter { $0.group == self } }
    }

    var group: Group {
        switch self {
        case .general, .chat, .coding, .archived: .basics
        case .providers, .usage: .models
        case .personalization, .subagents, .skills, .commands, .automations, .mcp, .search, .tools: .agent
        case .storage, .engine, .server: .system
        }
    }

    var title: String {
        switch self {
        case .general: "General"
        case .chat: "Chat"
        case .coding: "Coding"
        case .archived: "Archived Chats"
        case .providers: "Providers"
        case .usage: "Usage"
        case .commands: "Commands"
        case .automations: "Automations"
        case .personalization: "Personalization"
        case .subagents: "Subagents"
        case .skills: "Skills"
        case .mcp: "MCP Servers"
        case .search: "Web Search"
        case .tools: "Built-in Tools"
        case .storage: "Storage"
        case .engine: "Engine"
        case .server: "Server"
        }
    }

    var systemImage: String {
        switch self {
        case .general: "gearshape"
        case .chat: "bubble.left"
        case .coding: "chevron.left.forwardslash.chevron.right"
        case .archived: "archivebox"
        case .providers: "key"
        case .usage: "chart.bar"
        case .commands: "command"
        case .automations: "clock"
        case .personalization: "person"
        case .subagents: "person.2"
        case .skills: "sparkles"
        case .mcp: "powerplug"
        case .search: "globe"
        case .tools: "wrench.and.screwdriver"
        case .storage: "externaldrive"
        case .engine: "cpu"
        case .server: "server.rack"
        }
    }

    /// One short line under the page title.
    var subtitle: String {
        switch self {
        case .general: "Appearance and shortcuts."
        case .chat: "How chat turns behave."
        case .coding: "How workspaces behave."
        case .archived: "Chats hidden from the sidebar."
        case .providers: "Models you reach over the internet."
        case .usage: "Tokens spent, by model."
        case .commands: "Your own slash commands."
        case .automations: "Prompts that run on a schedule."
        case .personalization: "What every model knows about you."
        case .subagents: "Specialists the coder hands work to."
        case .skills: "Know-how a model loads when it fits."
        case .mcp: "Tools from other apps and services."
        case .search: "Where web searches go."
        case .tools: "What a model can call."
        case .storage: "Where model files live."
        case .engine: "How local models run."
        case .server: "Kuro's local API."
        }
    }

    /// The longer explanation, behind the question mark.
    var help: String? {
        switch self {
        case .providers:
            "Your key, your account, your bill. Requests go straight from this Mac to the provider. Kuro Free pools free-tier keys into one model."
        case .subagents:
            "A subagent gets a fresh context with only the task it was handed, works through it, and returns a short report. Read-only agents are given no tools that write."
        case .skills:
            "A turn ranks the enabled skills against the request and sends only what fits its budget."
        case .mcp:
            "Each server adds tools any model can call. They run on this Mac unless the server says otherwise."
        case .personalization:
            "Stored on this Mac. It leaves only with a message sent to a provider or free model."
        case .engine:
            "Changes apply the next time a model loads."
        case .commands:
            "Type / and the name in any message to drop the saved text in. {input} is replaced by whatever you type after the command."
        case .automations:
            "Each run sends the prompt to the automation's own chat. Times are this Mac's. A run missed while the Mac slept or Kuro was closed happens once when it is back."
        case .usage:
            "Counted from what each model reports. Some free tiers report nothing, so remote totals can be a floor."
        default: nil
        }
    }

    /// Extra words the settings search matches.
    var keywords: String {
        switch self {
        case .general: "theme dark light appearance keyboard shortcuts"
        case .chat: "effort orchestrate default"
        case .coding: "agent mode bypass permission workspace"
        case .archived: "archive restore hidden chats conversations"
        case .usage: "tokens usage billing stats cost"
        case .commands: "slash prompts templates snippets"
        case .automations: "schedule scheduled tasks recurring daily cron timer"
        case .providers: "api key openai anthropic openrouter groq free cloud gpu runpod endpoint models"
        case .personalization: "memory about you profile remember"
        case .subagents: "agents delegate reviewer explorer debugger tester architect specialist"
        case .skills: "import custom instructions"
        case .mcp: "servers connectors integrations"
        case .search: "duckduckgo brave tavily searxng web"
        case .tools: "builtin functions"
        case .storage: "folder disk models directory"
        case .engine: "context gpu layers threads unload memory"
        case .server: "restart stop log address port hardware loaded"
        }
    }

    func matches(_ query: String) -> Bool {
        let query = query.trimmingCharacters(in: .whitespaces).lowercased()
        return query.isEmpty || "\(title) \(subtitle) \(keywords)".lowercased().contains(query)
    }
}

/// Settings' own sidebar, in place of the app's while Settings are open.
struct SettingsSidebar: View {
    @Environment(AppModel.self) private var app
    let selection: SettingsPane
    @State private var query = ""

    var body: some View {
        // Only the list scrolls.
        VStack(alignment: .leading, spacing: 1) {
            SidebarRow("Back to app", systemImage: "arrow.left", selected: false) { app.leaveSettings() }
                .keyboardShortcut(.escape, modifiers: [])

            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass").foregroundStyle(.tertiary)
                TextField("Search settings", text: $query)
                    .textFieldStyle(.plain)
                    .onSubmit {
                        if let first = SettingsPane.allCases.first(where: { $0.matches(query) }) {
                            app.go(.settings(first))
                        }
                    }
            }
            .font(.system(size: 13))
            .padding(.horizontal, 8)
            .frame(height: 28)
            .background(Palette.hover, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(Palette.line))
            .padding(.top, 8)

            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    ForEach(SettingsPane.Group.allCases, id: \.self) { group in
                        let panes = group.panes.filter { $0.matches(query) }
                        if !panes.isEmpty {
                            SidebarHeading(group.rawValue)
                            ForEach(panes) { pane in
                                SidebarRow(pane.title, systemImage: pane.systemImage, selected: pane == selection) {
                                    app.go(.settings(pane))
                                }
                            }
                        }
                    }
                }
                .padding(.bottom, 8)
            }
            .scrollIndicators(.never)
        }
        .padding(.horizontal, 10)
        .padding(.top, 6)
    }
}

/// A settings page: the large title, then the page's own content.
struct SettingsPage<Content: View, Trailing: View>: View {
    let pane: SettingsPane
    /// Matches the content's own width, so the title lines up with it.
    var width: CGFloat = 760
    @ViewBuilder var trailing: Trailing
    @ViewBuilder var content: Content

    var body: some View {
        VStack(spacing: 0) {
            PageHeader(title: pane.title, subtitle: pane.subtitle, help: pane.help) { trailing }
                .padding(.horizontal, 20)
                .padding(.top, 28)
                .padding(.bottom, 4)
                .frame(maxWidth: width)
                .frame(maxWidth: .infinity)
            content
        }
    }
}

extension SettingsPage where Trailing == EmptyView {
    init(_ pane: SettingsPane, @ViewBuilder content: () -> Content) {
        self.init(pane: pane, trailing: { EmptyView() }, content: content)
    }
}
