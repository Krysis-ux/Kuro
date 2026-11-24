import SwiftUI
import KuroKit

/// The app's sidebar: a few places to go, recent chats, and Settings at the foot.
struct SidebarView: View {
    @Environment(AppModel.self) private var app
    @State private var renaming: Conversation?
    @State private var newTitle = ""
    @State private var deleting: Conversation?
    @State private var searching = false
    /// Project folders the user folded away.
    @State private var collapsed: Set<String> = []
    @FocusState private var searchFocused: Bool

    var body: some View {
        @Bindable var app = app
        let row = app.route.sidebarRow

        VStack(spacing: 0) {
            VStack(spacing: 1) {
                SidebarRow(title: "New chat", systemImage: "square.and.pencil", selected: row == .newChat, action: { app.go(.newChat) }) {
                    Text("⌘N").font(.caption).foregroundStyle(.tertiary)
                }
                SidebarRow("Code", systemImage: "chevron.left.forwardslash.chevron.right", selected: row == .code) { app.go(.code) }
                SidebarRow("Projects", systemImage: "folder", selected: row == .projects) { app.go(.projects) }
                SidebarRow("Models", systemImage: "cube", selected: row == .models) { app.go(.models) }
                SidebarRow("Automations", systemImage: "clock", selected: row == .settings(.automations)) { app.go(.settings(.automations)) }
            }

            SidebarHeading(title: "Recents") {
                Button {
                    searching.toggle()
                    if !searching { app.conversationQuery = "" }
                    searchFocused = searching
                } label: {
                    Image(systemName: searching ? "xmark" : "magnifyingglass")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .help(searching ? "Close search" : "Search chats")
            }

            if searching {
                TextField("Search chats", text: $app.conversationQuery)
                    .textFieldStyle(.plain)
                    .focused($searchFocused)
                    .font(.system(size: 13))
                    .padding(.horizontal, 8)
                    .frame(height: 28)
                    .background(Palette.hover, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(Palette.line))
                    .padding(.bottom, 4)
                    .onExitCommand { searching = false; app.conversationQuery = "" }
            }

            ScrollView {
                LazyVStack(spacing: 1) {
                    if app.conversations.isEmpty {
                        Text(app.conversationQuery.isEmpty ? "No chats yet" : "Nothing matched")
                            .font(.callout)
                            .foregroundStyle(.tertiary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 6)
                    }
                    if app.conversationQuery.isEmpty {
                        // Chats filed under a project sit in its folder, as in the project's own page.
                        ForEach(folders, id: \.project) { folder in
                            folderRow(folder)
                            if !collapsed.contains(folder.project) {
                                ForEach(folder.chats) { chatRow($0).padding(.leading, 16) }
                            }
                        }
                        ForEach(app.conversations.filter { $0.projectId == nil }) { chatRow($0) }
                    } else {
                        ForEach(app.conversations) { chatRow($0) }
                    }
                }
            }
            .scrollIndicators(.never)

            footer
        }
        .padding(.horizontal, 10)
        .padding(.top, 6)
        .task(id: app.conversationQuery) {
            try? await Task.sleep(for: .milliseconds(200))
            await app.loadConversations()
        }
        .alert("Rename Chat", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
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
            Text("It is removed from this Mac.")
        }
    }

    private struct Folder {
        let project: String
        let name: String
        let chats: [Conversation]
    }

    /// Projects that have chats, in the order of their latest chat.
    private var folders: [Folder] {
        var order: [String] = []
        var chats: [String: [Conversation]] = [:]
        for conversation in app.conversations {
            guard let project = conversation.projectId else { continue }
            if chats[project] == nil { order.append(project) }
            chats[project, default: []].append(conversation)
        }
        return order.map { id in
            Folder(project: id, name: app.projects.first { $0.id == id }?.name ?? "Project", chats: chats[id] ?? [])
        }
    }

    private func folderRow(_ folder: Folder) -> some View {
        let open = !collapsed.contains(folder.project)
        return SidebarRow(title: folder.name, systemImage: open ? "folder" : "folder.fill", selected: app.route == .project(folder.project), action: {
            withAnimation(.snappy) {
                if open { collapsed.insert(folder.project) } else { collapsed.remove(folder.project) }
            }
        }) {
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(.tertiary)
                .rotationEffect(.degrees(open ? 90 : 0))
        }
        .contextMenu {
            Button("Open Project") { app.go(.project(folder.project)) }
        }
    }

    private func chatRow(_ conversation: Conversation) -> some View {
        ConversationRow(
            conversation: conversation,
            running: app.turns.running.contains(conversation.id),
            selected: app.route == .chat(conversation.id)
        ) { app.go(.chat(conversation.id)) }
        .contextMenu {
            Button(conversation.pinned ? "Unpin" : "Pin") {
                Task { await app.pinConversation(conversation.id, !conversation.pinned) }
            }
            Menu("Move to Project") {
                ForEach(app.projects) { project in
                    Button(project.name) { Task { await app.moveConversation(conversation.id, toProject: project.id) } }
                        .disabled(project.id == conversation.projectId)
                }
                if app.projects.isEmpty { Button("No projects yet") {}.disabled(true) }
                if conversation.projectId != nil {
                    Divider()
                    Button("Remove from Project") { Task { await app.moveConversation(conversation.id, toProject: nil) } }
                }
            }
            Button("Rename…") {
                newTitle = conversation.title
                renaming = conversation
            }
            Divider()
            Button("Archive") { Task { await app.archiveConversation(conversation.id) } }
            Button("Delete…", role: .destructive) { deleting = conversation }
        }
    }

    private var footer: some View {
        HStack(spacing: 8) {
            KuroMark(size: 14)
                .frame(width: 26, height: 26)
                .background(Palette.raised, in: Circle())
                .overlay(Circle().strokeBorder(Palette.line))
            Text("Kuro").font(.system(size: 13, weight: .medium))
            StatusDot(color: app.server.phase == .ready ? .green : .secondary.opacity(0.4))
                .help(app.server.phase == .ready ? "Server running" : "Server not running")
            Spacer()
            Button { app.go(.settings(.general)) } label: {
                Image(systemName: "gearshape")
                    .font(.system(size: 14))
                    .foregroundStyle(.secondary)
                    .frame(width: 26, height: 26)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Settings (⌘,)")
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 10)
    }
}

private struct ConversationRow: View {
    let conversation: Conversation
    let running: Bool
    let selected: Bool
    let action: () -> Void

    var body: some View {
        SidebarRow(title: conversation.title, systemImage: nil, selected: selected, action: action) {
            if conversation.pinned {
                Image(systemName: "pin.fill").font(.system(size: 9)).foregroundStyle(.tertiary)
            }
            if running {
                ProgressView().controlSize(.mini)
            } else {
                // Short, as in "2h": the sidebar has no room for "ago".
                Text(Naming.relative(conversation.updatedAt).replacingOccurrences(of: " ago", with: "").replacingOccurrences(of: "just now", with: "now"))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
        .help(conversation.title)
    }
}
