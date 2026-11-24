import SwiftUI
import KuroKit

/// Chats hidden from the sidebar. Restore puts one back; Delete removes it for good.
struct ArchivedChats: View {
    @Environment(AppModel.self) private var app
    @State private var chats: [Conversation]?
    @State private var deleting: Conversation?
    @State private var failure: String?

    var body: some View {
        Form {
            Section {
                if let failure { ErrorText(failure) }
                if let chats {
                    if chats.isEmpty {
                        Text("Nothing archived. Right-click a chat in the sidebar to archive it.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(chats) { chat in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(chat.title).lineLimit(1)
                                Text(Naming.relative(chat.updatedAt)).font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button("Restore") {
                                act { try await app.client.setConversationFlags(chat.id, archived: false) }
                            }
                            Button {
                                deleting = chat
                            } label: {
                                Image(systemName: "trash")
                            }
                            .buttonStyle(.borderless)
                            .help("Delete for good")
                        }
                    }
                } else {
                    ProgressView()
                }
            }
        }
        .settingsForm()
        .task(id: app.stamp(.conversations)) { await load() }
        .confirmationDialog("Delete “\(deleting?.title ?? "")”?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete", role: .destructive) {
                guard let chat = deleting else { return }
                act { try await app.client.deleteConversation(chat.id) }
            }
        } message: {
            Text("It is removed from this Mac.")
        }
    }

    private func load() async {
        do {
            chats = try await app.client.archivedConversations()
            failure = nil
        } catch {
            failure = error.localizedDescription
        }
    }

    private func act(_ work: @escaping () async throws -> Void) {
        perform($failure) {
            try await work()
            app.invalidate(.conversations)
        }
    }
}
