import SwiftUI
import KuroKit

struct SidebarView: View {
    @Environment(AppModel.self) private var app
    @State private var renaming: Conversation?
    @State private var newTitle = ""
    @State private var deleting: Conversation?

    var body: some View {
        @Bindable var app = app
        let selection = Binding<Route?>(
            get: { app.route.sidebarRow },
            set: { if let route = $0, route != app.route.sidebarRow { app.go(route) } }
        )

        List(selection: selection) {
            Section {
                Label("New Chat", systemImage: "square.and.pencil").tag(Route.newChat)
                Label("Code", systemImage: "chevron.left.forwardslash.chevron.right").tag(Route.code)
                Label("Projects", systemImage: "folder").tag(Route.projects)
            }

            Section("Library") {
                Label("Models", systemImage: "cube").tag(Route.models)
                Label("Free Models", systemImage: "gift").tag(Route.free)
                Label("Providers", systemImage: "key").tag(Route.providers)
                Label("Cloud", systemImage: "cloud").tag(Route.cloud)
                Label("Tools", systemImage: "wrench.and.screwdriver").tag(Route.tools)
            }

            Section("Recents") {
                if app.conversations.isEmpty {
                    Text(app.conversationQuery.isEmpty ? "No conversations yet" : "Nothing matched")
                        .foregroundStyle(.tertiary)
                }
                ForEach(app.conversations) { conversation in
                    ConversationRow(conversation: conversation, running: app.turns.running.contains(conversation.id))
                        .tag(Route.chat(conversation.id))
                        .contextMenu {
                            Button("Rename…") {
                                newTitle = conversation.title
                                renaming = conversation
                            }
                            Divider()
                            Button("Delete…", role: .destructive) { deleting = conversation }
                        }
                }
            }
        }
        .listStyle(.sidebar)
        .searchable(text: $app.conversationQuery, placement: .sidebar, prompt: "Search chats")
        .task(id: app.conversationQuery) {
            try? await Task.sleep(for: .milliseconds(200))
            await app.loadConversations()
        }
        .safeAreaInset(edge: .bottom) { footer }
        .alert("Rename Conversation", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Title", text: $newTitle)
            Button("Rename") {
                if let renaming { Task { await app.renameConversation(renaming.id, to: newTitle) } }
            }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog(
            "Delete “\(deleting?.title ?? "")”?",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })
        ) {
            Button("Delete", role: .destructive) {
                if let deleting { Task { await app.deleteConversation(deleting.id) } }
            }
        } message: {
            Text("The conversation and its messages are removed from this Mac.")
        }
    }

    private var footer: some View {
        HStack(spacing: 8) {
            KuroMark(size: 16)
            Text("Kuro").font(.headline)
            Spacer()
            SettingsLink {
                Image(systemName: "gearshape")
            }
            .buttonStyle(.borderless)
            .help("Settings")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }
}

private struct ConversationRow: View {
    let conversation: Conversation
    let running: Bool

    var body: some View {
        HStack(spacing: 6) {
            Text(conversation.title)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
            if running {
                ProgressView().controlSize(.mini)
            } else {
                Text(Naming.relative(conversation.updatedAt))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .help(conversation.title)
    }
}
