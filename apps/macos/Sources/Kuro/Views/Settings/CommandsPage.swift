import SwiftUI
import KuroKit

/// The user's own slash commands: saved text that `/name` drops into a message.
struct CommandsPage: View {
    @Environment(AppModel.self) private var app
    @State private var editing: EditRequest?
    @State private var failure: String?

    struct EditRequest: Identifiable {
        let id = UUID()
        let command: CustomCommand?
    }

    var body: some View {
        let commands = app.customCommands

        SettingsPage(pane: .commands) {
            Button { editing = EditRequest(command: nil) } label: { Label("New", systemImage: "plus") }
                .buttonStyle(PrimaryButtonStyle())
        } content: {
            Form {
                Section {
                    if let failure { ErrorText(failure) }
                    if commands.isEmpty {
                        Text("None yet. Save a prompt you type often, then use it with /name.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(commands) { command in
                        HStack(alignment: .firstTextBaseline) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("/\(command.name)").font(.body.monospaced())
                                Text(command.hint.isEmpty ? command.text : command.hint)
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            Spacer()
                            Button("Edit") { editing = EditRequest(command: command) }
                            Button { remove(command) } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                                .help("Delete")
                        }
                    }
                } header: {
                    Text("Yours")
                }
            }
            .settingsForm()
        }
        .task { await app.loadCustomCommands() }
        .sheet(item: $editing) { request in
            CommandEditor(original: request.command) { editing = nil }
        }
    }

    private func remove(_ command: CustomCommand) {
        perform($failure) {
            try await app.saveCustomCommands(app.customCommands.filter { $0.name != command.name })
        }
    }
}

private struct CommandEditor: View {
    @Environment(AppModel.self) private var app
    let original: CustomCommand?
    let onClose: () -> Void

    @State private var name = ""
    @State private var hint = ""
    @State private var text = ""
    @State private var failure: String?
    @State private var saving = false

    /// Switches the composer already answers to.
    private static let reserved: Set<String> = ["web", "folders"]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(original == nil ? "New Command" : "Edit Command")
                .font(.system(size: 16, weight: .semibold))
                .padding([.horizontal, .top], 20)
                .padding(.bottom, 12)

            Form {
                Section {
                    TextField(text: $name, prompt: Text("explain")) {
                        TitleWithHelp("Name", help: "Typed after a slash. Letters, digits and dashes.")
                    }
                    TextField("Description", text: $hint, prompt: Text("Explain this like I'm new to it"))
                }
                Section {
                    TextEditor(text: $text)
                        .font(.body)
                        .frame(minHeight: 140)
                        .scrollContentBackground(.hidden)
                } header: {
                    TitleWithHelp("Text", help: "Put {input} where the caret should land, so you can keep typing there.")
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)

            HStack {
                if let failure { ErrorText(failure).lineLimit(2) }
                Spacer()
                Button("Cancel", action: onClose)
                    .buttonStyle(SubtleButtonStyle())
                    .keyboardShortcut(.cancelAction)
                Button(saving ? "Saving…" : "Save", action: save)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(slug.isEmpty || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || saving)
            }
            .padding(20)
        }
        .frame(width: 500, height: 460)
        .background(Palette.canvas)
        .onAppear {
            name = original?.name ?? ""
            hint = original?.hint ?? ""
            text = original?.text ?? ""
        }
    }

    private var slug: String { CustomCommand.normalise(name) }

    private func save() {
        if Self.reserved.contains(slug) {
            failure = "/\(slug) is already a switch in the message box."
            return
        }
        let others = app.customCommands.filter { $0.name != original?.name }
        if others.contains(where: { $0.name == slug }) {
            failure = "You already have /\(slug)."
            return
        }
        let command = CustomCommand(name: slug, hint: hint.trimmingCharacters(in: .whitespaces), text: text)
        // Kept in place when edited, so the list does not reorder under the user.
        var updated = app.customCommands
        if let original, let index = updated.firstIndex(where: { $0.name == original.name }) {
            updated[index] = command
        } else {
            updated.append(command)
        }
        saving = true
        perform($failure) {
            defer { saving = false }
            try await app.saveCustomCommands(updated)
            onClose()
        }
    }
}
