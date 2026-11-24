import AppKit
import SwiftUI
import KuroKit

/// The page beside the Settings sidebar. Chat and Coding are configured
/// separately on purpose: a chat that ran commands would be alarming, a coding
/// turn that could not is useless.
struct SettingsDetail: View {
    @Environment(AppModel.self) private var app
    let pane: SettingsPane

    @State private var settings: [String: JSONValue] = [:]
    @State private var hardware: HardwareInfo?
    @State private var failure: String?

    var body: some View {
        page
            .overlay(alignment: .bottom) {
                if let failure { ErrorText(failure).padding(8).background(.bar, in: Capsule()).padding() }
            }
            .navigationTitle("Settings")
            .task(id: app.server.phase) { await load() }
    }

    @ViewBuilder private var page: some View {
        switch pane {
        case .general: SettingsPage(pane) { GeneralSettings() }
        case .chat: SettingsPage(pane) { ChatSettings(save: save) }
        case .coding: SettingsPage(pane) { CoderSettings(save: save) }
        case .archived: SettingsPage(pane) { ArchivedChats() }
        case .usage: SettingsPage(pane) { UsageSettings() }
        case .commands: CommandsPage()
        case .automations: AutomationsPage()
        case .providers: ProvidersPage()
        case .personalization: SettingsPage(pane) { PersonalizationSettings(settings: settings, save: save) }
        case .subagents: SubagentsPage()
        case .skills, .mcp, .search, .tools: toolsPane
        case .storage: SettingsPage(pane) { StorageSettings(settings: settings, save: save) }
        case .engine: SettingsPage(pane) { EngineSettings(settings: settings, hardware: hardware, save: save) }
        case .server: SettingsPage(pane) { ServerSettings(hardware: hardware) }
        }
    }

    @ViewBuilder private var toolsPane: some View {
        if let overview = app.tools {
            switch pane {
            case .skills: SettingsPage(pane) { SkillsPane(overview: overview) }
            case .mcp: McpPane()
            case .search: SettingsPage(pane) { SearchPane(overview: overview) }
            default: SettingsPage(pane) { BuiltinPane(overview: overview) }
            }
        } else {
            VStack(spacing: 10) {
                ProgressView()
                Button("Try Again") { app.invalidate(.tools) }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func load() async {
        guard app.server.phase == .ready else { return }
        settings = (try? await app.client.settings()) ?? settings
        hardware = try? await app.client.hardware()
    }

    /// `nil` values clear an override and return the setting to automatic.
    private func save(_ patch: KuroClient.Body) {
        perform($failure) {
            try await app.client.patchSettings(patch)
            await load()
            app.invalidate(.tools)
        }
    }
}

// MARK: - General

private struct GeneralSettings: View {
    @Environment(AppModel.self) private var app

    private let shortcuts: [(String, String)] = [
        ("New chat", "⌘N"),
        ("Settings", "⌘,"),
        ("Code", "⌥⌘1"),
        ("Projects", "⌥⌘2"),
        ("Models", "⌥⌘3"),
        ("Providers", "⌥⌘4"),
        ("Send", "↩"),
        ("New line", "⇧↩"),
        ("Slash commands", "/"),
    ]

    var body: some View {
        @Bindable var prefs = app.prefs
        Form {
            Section("Appearance") {
                Picker("Theme", selection: $prefs.theme) {
                    Text("System").tag(Preferences.Theme.system)
                    Text("Light").tag(Preferences.Theme.light)
                    Text("Dark").tag(Preferences.Theme.dark)
                }
                .pickerStyle(.segmented)
            }
            Section("Keyboard Shortcuts") {
                ForEach(shortcuts, id: \.0) { name, keys in
                    LabeledContent(name) {
                        Text(keys).font(.callout.monospaced()).foregroundStyle(.secondary)
                    }
                }
            }
        }
        .settingsForm()
    }
}

// MARK: - Chat

private struct ChatSettings: View {
    @Environment(AppModel.self) private var app
    let save: (KuroClient.Body) -> Void

    var body: some View {
        Form {
            if let surface = app.tools?.surfaces.chat {
                SurfaceSection(surface: surface, autoKey: "chat.autoOrchestrate", effortKey: "chat.defaultEffort", save: save,
                               autoHint: "Higher effort also picks reasoning guidance that suits the question, and gives the model more room to search.")
            }
        }
        .settingsForm()
    }
}

// MARK: - Personalization

private struct PersonalizationSettings: View {
    @Environment(AppModel.self) private var app
    let settings: [String: JSONValue]
    let save: (KuroClient.Body) -> Void
    @State private var aboutYou = ""
    @State private var loaded = false

    var body: some View {
        let stored = settings["memory.aboutYou"]?.stringValue ?? ""

        VStack(spacing: 0) {
            Form {
                Section {
                    TextEditor(text: $aboutYou)
                        .frame(minHeight: 90)
                        .scrollContentBackground(.hidden)
                    HStack {
                        Text(aboutYou == stored ? "Sent with every conversation." : "Not saved yet.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Button("Save") { save(["memory.aboutYou": aboutYou]) }
                            .disabled(aboutYou == stored)
                    }
                } header: {
                    TitleWithHelp("About You", help: "Your work, your stack, how you like answers. Anything you would otherwise retype.")
                }
            }
            .settingsForm()
            .frame(height: 210)

            if let overview = app.tools { MemoryPane(overview: overview) }
        }
        .onChange(of: stored, initial: true) {
            // Adopt the stored text until the field is edited.
            if !loaded || aboutYou == "" { aboutYou = stored }
            loaded = true
        }
    }
}

/// One surface's settings: whether effort may do more than lengthen answers, and where it starts.
private struct SurfaceSection: View {
    let surface: SurfaceSettings
    let autoKey: String
    let effortKey: String
    let save: (KuroClient.Body) -> Void
    let autoHint: String

    var body: some View {
        Section {
            Toggle(isOn: Binding(get: { surface.autoOrchestrate }, set: { save([autoKey: $0]) })) {
                TitleWithHelp("Auto-orchestrate", help: autoHint)
            }
            Picker(selection: Binding(get: { surface.defaultEffort }, set: { save([effortKey: $0.rawValue]) })) {
                ForEach(Effort.options(coding: false), id: \.self) { Text($0.label).tag($0) }
            } label: {
                TitleWithHelp("Starting effort", help: "Where the effort control beside the message box starts. It can still be changed per message.")
            }
            .pickerStyle(.segmented)
        }
    }
}

// MARK: - Coder

private struct CoderSettings: View {
    @Environment(AppModel.self) private var app
    let save: (KuroClient.Body) -> Void

    var body: some View {
        Form {
            if let tools = app.tools {
                let surface = tools.surfaces.code
                SurfaceSection(surface: surface, autoKey: "code.autoOrchestrate", effortKey: "code.defaultEffort", save: save,
                               autoHint: "Higher effort buys more rounds of reading, editing and running, and brings in the skills that match what the project is written in.")

                Section {
                    Picker("New workspaces start in", selection: Binding(get: { surface.defaultMode ?? .agent }, set: { save(["code.defaultMode": $0.rawValue]) })) {
                        ForEach(WorkspaceMode.allCases, id: \.self) { mode in
                            VStack(alignment: .leading, spacing: 1) {
                                Text(mode.rawValue.capitalized)
                                Text(mode.note).font(.caption).foregroundStyle(.secondary)
                            }
                            .tag(mode)
                        }
                    }
                    .pickerStyle(.radioGroup)
                } header: {
                    TitleWithHelp("Permission", help: "Agent is the default because every file it changes can be undone from the Changes panel. A command that has already run cannot be undone, which is why Bypass is never the default.")
                }

                Section {
                    ForEach(tools.skills.essentials) { skill in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(skill.name)
                            Text(skill.blurb).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                } header: {
                    TitleWithHelp("Always On When Coding", help: "Everything else is a choice, under Skills.")
                }
            }
        }
        .settingsForm()
    }
}

// MARK: - Storage

private struct StorageSettings: View {
    @Environment(AppModel.self) private var app
    let settings: [String: JSONValue]
    let save: (KuroClient.Body) -> Void
    @State private var status: ServerStatus?

    var body: some View {
        let custom = settings["models.directory"]?.stringValue ?? ""
        Form {
            Section {
                LabeledContent("Models folder") {
                    Text(custom.isEmpty ? "\(status?.dataDirectory ?? "—") (default)" : custom)
                        .font(.callout.monospaced())
                        .lineLimit(2)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                }
                HStack {
                    Spacer()
                    if !custom.isEmpty { Button("Use Default") { save(["models.directory": ""]) } }
                    Button("Choose…") {
                        if let path = Pick.folder(title: "Choose where to keep model files", startingAt: custom) {
                            save(["models.directory": path])
                        }
                    }
                }
            } header: {
                TitleWithHelp("Where Models Are Kept", help: "Point this at an external drive if your disk is tight. Restart the server after changing it. Existing downloads stay where they are.")
            }
        }
        .settingsForm()
        .task { status = try? await app.client.status() }
    }
}

// MARK: - Engine

private struct EngineSettings: View {
    let settings: [String: JSONValue]
    let hardware: HardwareInfo?
    let save: (KuroClient.Body) -> Void

    var body: some View {
        Form {
            Section {
                EngineSlider(label: "Context size", key: "engine.contextSize", unit: "tokens", auto: hardware?.recommended.contextSize,
                             range: 512...131_072, step: 512, settings: settings, save: save,
                             hint: "How much the model sees at once. More costs memory.")
                EngineSlider(label: "GPU layers", key: "engine.gpuLayers", auto: hardware?.recommended.gpuLayers,
                             range: 0...999, step: 1, zeroLabel: "CPU only", maxLabel: "All", settings: settings, save: save,
                             hint: "Layers on the GPU. All is almost always right.")
                EngineSlider(label: "CPU threads", key: "engine.threads", auto: hardware?.recommended.threads,
                             range: 1...Double(max(hardware?.logicalCores ?? 16, 2)), step: 1, settings: settings, save: save,
                             hint: "This Mac has \(hardware?.physicalCores ?? 0) physical cores.")
                EngineSlider(label: "Unload after", key: "engine.idleUnloadMinutes", unit: "min", auto: 30,
                             range: 0...240, step: 5, zeroLabel: "never unload", settings: settings, save: save,
                             hint: "How long an idle model stays in memory.")
            }
        }
        .settingsForm()
    }
}

/// A number with a slider and an exact field. Saved when the drag ends, not on every step.
private struct EngineSlider: View {
    let label: String
    let key: String
    var unit: String?
    let auto: Int?
    let range: ClosedRange<Double>
    let step: Double
    var zeroLabel: String?
    /// Said instead of the top of the range, where that is a sentinel rather than a count.
    var maxLabel: String?
    let settings: [String: JSONValue]
    let save: (KuroClient.Body) -> Void
    let hint: String

    @State private var draft: Double = 0
    @State private var typed = ""
    @State private var dragging = false

    private var stored: Double? {
        if case .number(let value) = settings[key] { return value }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(label)
                Spacer()
                TextField("Value", text: $typed, prompt: Text(stored == nil ? "Auto · \(format(auto.map(Double.init)))" : format(stored)))
                    .labelsHidden()
                    .multilineTextAlignment(.trailing)
                    .frame(width: 170)
                    .onSubmit {
                        if let value = Double(typed.filter { $0.isNumber || $0 == "." }) {
                            commit(min(max(value, range.lowerBound), range.upperBound))
                        }
                        typed = ""
                    }
            }
            // No `step:` here: it draws a tick per step, which is a solid smear for context sizes.
            Slider(value: $draft, in: range) { editing in
                dragging = editing
                if !editing { commit(draft) }
            }
            HStack {
                Text(hint).font(.caption).foregroundStyle(.secondary)
                Spacer()
                if stored != nil {
                    Button("Reset to Auto") { save([key: nil]) }.buttonStyle(.link).font(.caption)
                }
            }
        }
        .onChange(of: stored, initial: true) { syncDraft() }
        .onChange(of: auto) { syncDraft() }
    }

    private func syncDraft() {
        if !dragging { draft = stored ?? Double(auto ?? Int(range.lowerBound)) }
    }

    private func commit(_ value: Double) {
        let snapped = min(max((value / step).rounded() * step, range.lowerBound), range.upperBound)
        draft = snapped
        save([key: Int(snapped)])
    }

    private func format(_ value: Double?) -> String {
        guard let value else { return "—" }
        if value == 0, let zeroLabel { return zeroLabel }
        if value >= range.upperBound, let maxLabel { return maxLabel }
        let number = Int(value).formatted()
        return unit.map { "\(number) \($0)" } ?? number
    }
}

// MARK: - Server

private struct ServerSettings: View {
    @Environment(AppModel.self) private var app
    let hardware: HardwareInfo?

    @State private var status: ServerStatus?
    @State private var restarting = false
    @State private var confirmStop = false
    @State private var failure: String?

    var body: some View {
        Form {
            Section("Server") {
                LabeledContent("Status", value: status == nil ? "Unreachable" : "Running")
                LabeledContent("Address") { Text(status?.address ?? "—").monospaced().textSelection(.enabled) }
                LabeledContent("Uptime", value: status.map { Naming.uptime($0.uptimeSeconds) } ?? "—")
                LabeledContent("Version") { Text(status?.version ?? "—").monospaced() }
                LabeledContent("Data") {
                    HStack {
                        Text(status?.dataDirectory ?? "—").font(.callout.monospaced()).lineLimit(1).truncationMode(.middle)
                        if let path = status?.dataDirectory {
                            Button { NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: path) } label: { Image(systemName: "folder") }
                                .buttonStyle(.borderless)
                                .help("Show in Finder")
                        }
                    }
                }
                if let failure { ErrorText(failure) }
                HStack {
                    Button("Show Log") { NSWorkspace.shared.open(ServerController.logURL) }
                        .disabled(!app.server.owned)
                    Spacer()
                    Button(restarting ? "Restarting…" : "Restart") {
                        restarting = true
                        perform($failure) {
                            defer { restarting = false }
                            try await app.server.restart()
                        }
                    }
                    .disabled(restarting)
                    .help("Applies engine and storage settings, and clears a stuck engine")
                    Button("Stop Server…", role: .destructive) { confirmStop = true }
                }
            }

            Section("Loaded Models") {
                if status?.loadedModels.isEmpty ?? true {
                    Text("None").foregroundStyle(.secondary)
                }
                ForEach(status?.loadedModels ?? []) { engine in
                    HStack {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(engine.modelId).monospaced()
                            Text("port \(engine.port) · pid \(engine.pid) · idle \(engine.idleSeconds)s")
                                .font(.caption.monospaced())
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Unload") {
                            perform($failure) {
                                try await app.client.unloadModel(engine.modelId)
                                status = try await app.client.status()
                                app.invalidate(.models)
                            }
                        }
                    }
                }
            }

            Section("Hardware") {
                LabeledContent("Chip", value: hardware?.chip ?? "—")
                LabeledContent("Memory", value: Naming.bytes(hardware?.totalMemoryBytes))
                LabeledContent("Cores", value: hardware.map { "\($0.physicalCores) physical · \($0.logicalCores) logical" } ?? "—")
                LabeledContent("GPU", value: hardware.map { $0.gpuAvailable ? $0.gpuBackend : "CPU only" } ?? "—")
            }
        }
        .settingsForm()
        .task {
            while !Task.isCancelled {
                status = try? await app.client.status()
                try? await Task.sleep(for: .seconds(5))
            }
        }
        .confirmationDialog("Stop Kuro's server?", isPresented: $confirmStop) {
            Button("Stop Server", role: .destructive) { Task { await app.server.stop() } }
        } message: {
            Text("Models unload and the API closes for every app using it.")
        }
    }
}
