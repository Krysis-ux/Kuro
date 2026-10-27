import SwiftUI
import UniformTypeIdentifiers
import KuroKit

/// A switch beside the message box (Web, Folders).
struct ComposerToggle: Identifiable {
    let id: String
    /// Its name in the `/` palette.
    let command: String
    let label: String
    let systemImage: String
    let isOn: Binding<Bool>
    let help: String
}

/// The message box, shared by Chat and Code. What differs between the two is
/// which switches sit on the left.
struct Composer: View {
    @Environment(AppModel.self) private var app

    /// Where the half-written message is kept.
    let draftKey: String
    /// Unlocks Ultracode and coding wording.
    let coding: Bool
    @Binding var model: String?
    @Binding var effort: Effort
    var effortNote: String
    var toggles: [ComposerToggle] = []
    /// Shown before the toggles: the Code page's mode switch. Told whether space is tight.
    var leading: ((_ compact: Bool) -> AnyView)?
    var placeholder: String?
    var hint: String?
    /// Set when sending is impossible, with the reason.
    var disabledReason: String?
    let isRunning: Bool
    /// `skills` are the ones named with `/` in the message.
    let onSend: (_ content: String, _ skills: [String]) -> Void
    let onStop: () -> Void

    private struct Attachment: Identifiable {
        let id = UUID()
        let name: String
        let content: String
    }

    @State private var text = ""
    @State private var caret = 0
    @State private var caretRequest: Int?
    @State private var inputHeight: CGFloat = 22
    /// The `/query` the user dismissed with Escape; typing on reopens the menu.
    @State private var dismissed: String?
    @State private var slashIndex = 0
    @State private var attachments: [Attachment] = []
    @State private var attachError: String?
    @State private var servers: [McpServer] = []

    var body: some View {
        let token = Slash.token(in: text, caret: caret)
        let matches = token.map { Slash.match(commands, query: $0.query) } ?? []
        let slashOpen = token != nil && !matches.isEmpty && dismissed != token?.query

        VStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 8) {
                chips
                if let attachError { ErrorText(attachError) }

                ZStack(alignment: .topLeading) {
                    if text.isEmpty {
                        Text(placeholder ?? defaultPlaceholder)
                            .font(.system(size: 14))
                            .foregroundStyle(.tertiary)
                            .padding(.leading, 4)
                            .padding(.top, 2)
                            .allowsHitTesting(false)
                    }
                    ComposerTextView(
                        text: $text,
                        caret: $caret,
                        caretRequest: $caretRequest,
                        height: $inputHeight,
                        onKey: { handle($0, token: token, matches: matches, open: slashOpen) }
                    )
                    .frame(height: min(max(inputHeight, 20), 260))
                }

                ViewThatFits(in: .horizontal) {
                    actionRow(compact: false)
                    actionRow(compact: true)
                }
            }
            .padding(.horizontal, 14)
            .padding(.top, 12)
            .padding(.bottom, 10)
            .glassCard(cornerRadius: 22)
            .overlay(alignment: .top) {
                if slashOpen {
                    SlashMenu(commands: matches, index: min(slashIndex, matches.count - 1), onHover: { slashIndex = $0 }) {
                        run($0, token: token)
                    }
                    .alignmentGuide(.top) { $0[.bottom] + 8 }
                    .transition(.opacity)
                }
            }

            Text(disabledReason ?? hint ?? defaultHint)
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
        }
        .onAppear { text = app.prefs.draft(draftKey) }
        .onChange(of: text) { app.prefs.setDraft(draftKey, text) }
        .onChange(of: token?.query) { slashIndex = 0 }
        .task { servers = (try? await app.client.mcpServers(connect: false)) ?? [] }
    }

    // MARK: Pieces

    @ViewBuilder private var chips: some View {
        let named = namedSkills
        if !named.isEmpty || !attachments.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    // The `/name` in the text is the attachment; removing the chip removes the word.
                    ForEach(named, id: \.self) { slug in
                        chip("/\(slug)", systemImage: "sparkles", live: true) {
                            text = Slash.removing(slug, from: text)
                            caret = min(caret, text.utf16.count)
                        }
                    }
                    ForEach(attachments) { file in
                        chip(file.name, systemImage: "doc.text", live: false) {
                            attachments.removeAll { $0.id == file.id }
                        }
                    }
                }
            }
        }
    }

    private func chip(_ title: String, systemImage: String, live: Bool, remove: @escaping () -> Void) -> some View {
        HStack(spacing: 4) {
            Image(systemName: systemImage)
            Text(title).lineLimit(1)
            Button(action: remove) { Image(systemName: "xmark").font(.caption2.weight(.bold)) }
                .buttonStyle(.plain)
                .help("Remove")
        }
        .font(.caption)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .foregroundStyle(live ? Color.accentColor : .secondary)
        .background((live ? Color.accentColor : Color.primary).opacity(0.1), in: Capsule())
    }

    private func actionRow(compact: Bool) -> some View {
        HStack(spacing: 4) {
            addMenu
            if let leading { leading(compact) }
            ForEach(toggles) { toggle in
                Toggle(isOn: toggle.isOn) {
                    if compact {
                        Image(systemName: toggle.systemImage)
                    } else {
                        Label(toggle.label, systemImage: toggle.systemImage)
                    }
                }
                .toggleStyle(PillToggleStyle())
                .help(toggle.help)
            }
            EffortPicker(effort: $effort, coding: coding, note: effortNote, compact: compact)
            Spacer(minLength: 8)
            ModelPicker(selection: $model)
            sendButton
        }
    }

    private var addMenu: some View {
        Menu {
            Section("Attach") {
                Button("Text and Code Files…", systemImage: "doc.text") { attachFiles() }
            }
            Section("Not Supported Yet") {
                Button("Images", systemImage: "photo") {}.disabled(true)
                Button("Audio", systemImage: "mic") {}.disabled(true)
                Button("Video", systemImage: "video") {}.disabled(true)
                Button("PDF", systemImage: "doc.richtext") {}.disabled(true)
            }
            Section("Tools") {
                let connected = servers.filter { $0.enabled && $0.status == "connected" }
                let count = connected.reduce(0) { $0 + ($1.toolCount ?? $1.tools.count) }
                Button(count > 0 ? "Tools and MCP (\(count))" : "Tools and MCP", systemImage: "wrench.and.screwdriver") {
                    app.go(.tools, returning: true)
                }
                ForEach(connected.prefix(4)) { server in
                    Button(server.name, systemImage: "powerplug") { app.go(.tools, returning: true) }
                }
                Button("Open a Folder in Code", systemImage: "folder") { app.go(.code, returning: true) }
            }
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 13, weight: .semibold))
                .frame(width: 26, height: 26)
                .contentShape(Circle())
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Attach files or open tools")
    }

    private var sendButton: some View {
        let ready = canSend
        return Button {
            isRunning ? onStop() : submit()
        } label: {
            Image(systemName: isRunning ? "stop.fill" : "arrow.up")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(Color(nsColor: .windowBackgroundColor))
                .frame(width: 28, height: 28)
                .background(Circle().fill(ready || isRunning ? Color.primary : Color.secondary.opacity(0.35)))
                .contentTransition(.symbolEffect(.replace))
        }
        .buttonStyle(.plain)
        .disabled(!isRunning && !ready)
        .help(disabledReason ?? (isRunning ? "Stop" : "Send"))
    }

    // MARK: Behaviour

    private var activeModel: String? { model ?? app.models.fallbackId() }
    private var isRemote: Bool { activeModel.map(Naming.isRemote) ?? false }

    private var defaultPlaceholder: String {
        isRemote ? "Message — this one goes to your provider…" : "Message Kuro…"
    }

    private var defaultHint: String {
        isRemote
            ? "Return to send, Shift-Return for a new line. This model runs on your provider, not here."
            : "Return to send, Shift-Return for a new line. Models run entirely on this Mac."
    }

    private var canSend: Bool {
        !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && disabledReason == nil
    }

    private var skillCatalogue: [Skill] { app.tools?.skills.catalogue ?? [] }

    private var namedSkills: [String] {
        Slash.skillsNamed(in: text, known: Set(skillCatalogue.map(\.slug)))
    }

    /// Switches first: a handful that change what this message does, ahead of forty skills.
    private var commands: [SlashCommand] {
        let named = namedSkills
        let switches = toggles.map { toggle in
            SlashCommand(
                name: toggle.command,
                hint: "Turn \(toggle.label.lowercased()) \(toggle.isOn.wrappedValue ? "off" : "on") for this message",
                state: toggle.isOn.wrappedValue ? "on" : "off",
                kind: .toggle
            )
        }
        let skills = skillCatalogue.map { skill in
            SlashCommand(
                name: skill.slug,
                hint: skill.blurb,
                aliases: [skill.name.lowercased()],
                state: named.contains(skill.slug) ? "in message" : nil,
                kind: .skill
            )
        }
        var seen = Set<String>()
        return (switches + skills).filter { seen.insert($0.name).inserted }
    }

    private func handle(_ key: ComposerKey, token: CommandToken?, matches: [SlashCommand], open: Bool) -> Bool {
        if open, let token {
            let index = min(slashIndex, matches.count - 1)
            switch key {
            case .down: slashIndex = (index + 1) % matches.count
            case .up: slashIndex = (index - 1 + matches.count) % matches.count
            // Tab completes the name without running it.
            case .tab: replace(token, with: "/\(matches[index].name)")
            case .submit: run(matches[index], token: token)
            // Close the menu and leave the message alone.
            case .escape: dismissed = token.query
            }
            return true
        }
        guard key == .submit else { return false }
        submit()
        return true
    }

    private func run(_ command: SlashCommand, token: CommandToken?) {
        guard let token else { return }
        switch command.kind {
        case .skill:
            // The name stays in the sentence so the model knows what the guidance is for.
            replace(token, with: "/\(command.name) ")
        case .toggle:
            // A switch configures the turn; its name would only be noise in the message.
            replace(token, with: "")
            toggles.first { $0.command == command.name }?.isOn.wrappedValue.toggle()
        }
        dismissed = nil
    }

    private func replace(_ token: CommandToken, with replacement: String) {
        let result = Slash.replace(token, in: text, with: replacement)
        text = result.text
        caret = result.caret
        caretRequest = result.caret
    }

    private func submit() {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !isRunning, disabledReason == nil else { return }
        // Attached files go first, so the model reads them before the question.
        let preamble = attachments.map { "--- \($0.name) ---\n\($0.content)" }.joined(separator: "\n\n")
        onSend(preamble.isEmpty ? trimmed : "\(preamble)\n\n\(trimmed)", namedSkills)
        text = ""
        attachments = []
        attachError = nil
    }

    private func attachFiles() {
        attachError = nil
        var rejected: [String] = []
        for url in Pick.files(title: "Attach text or code", types: [], multiple: true) {
            guard let data = try? Data(contentsOf: url) else {
                rejected.append(url.lastPathComponent)
                continue
            }
            let content = String(decoding: data, as: UTF8.self)
            if looksBinary(content) {
                rejected.append(url.lastPathComponent)
            } else {
                attachments.append(Attachment(name: url.lastPathComponent, content: content))
            }
        }
        if !rejected.isEmpty {
            attachError = "\(rejected.joined(separator: ", ")) \(rejected.count == 1 ? "is" : "are") not readable as text."
        }
    }
}

/// The `/` palette. Keys arrive in the text view, so selection lives in the composer.
private struct SlashMenu: View {
    let commands: [SlashCommand]
    let index: Int
    let onHover: (Int) -> Void
    let onRun: (SlashCommand) -> Void

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 1) {
                    ForEach(Array(commands.enumerated()), id: \.element.id) { position, command in
                        HStack(spacing: 10) {
                            Text("/\(command.name)")
                                .font(.callout.monospaced().weight(.medium))
                            Text(command.hint)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                            Spacer(minLength: 8)
                            if let state = command.state {
                                Tag(state, tone: state == "off" ? .neutral : .live)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(position == index ? Color.accentColor.opacity(0.16) : .clear, in: RoundedRectangle(cornerRadius: 8))
                        .contentShape(Rectangle())
                        .onTapGesture { onRun(command) }
                        .onHover { if $0 { onHover(position) } }
                        .id(position)
                    }
                }
                .padding(5)
            }
            .frame(maxHeight: 280)
            .fixedSize(horizontal: false, vertical: true)
            .onChange(of: index) { proxy.scrollTo(index) }
        }
        .glassCard(cornerRadius: 14)
    }
}
