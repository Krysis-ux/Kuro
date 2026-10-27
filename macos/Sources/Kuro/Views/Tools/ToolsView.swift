import SwiftUI
import KuroKit

/// What a model can do besides produce text: Kuro's own tools and skills, and
/// MCP servers. From the model's side these are the same thing.
struct ToolsView: View {
    @Environment(AppModel.self) private var app

    enum Pane: String, CaseIterable {
        case skills = "Skills"
        case search = "Web Search"
        case memory = "Memory"
        case mcp = "MCP Servers"
        case builtin = "Built In"
    }

    @State private var pane: Pane = .skills

    var body: some View {
        Group {
            if let overview = app.tools {
                switch pane {
                case .skills: SkillsPane(overview: overview)
                case .search: SearchPane(overview: overview)
                case .memory: MemoryPane(overview: overview)
                case .mcp: McpPane()
                case .builtin: BuiltinPane(overview: overview)
                }
            } else {
                VStack(spacing: 10) {
                    ProgressView()
                    Button("Try Again") { app.invalidate(.tools) }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle("Tools")
        .toolbar {
            BackToolbarItem()
            ToolbarItem(placement: .principal) {
                Picker("Section", selection: $pane) {
                    ForEach(Pane.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }
        }
    }
}

// MARK: - Web search

private struct SearchPane: View {
    @Environment(AppModel.self) private var app
    let overview: ToolsOverview

    @State private var key = ""
    @State private var baseUrl = ""
    @State private var testing = false
    @State private var result: SearchTest?
    @State private var failure: String?

    var body: some View {
        let search = overview.search
        let selected = search.providers.first { $0.id == search.provider }

        Form {
            Section {
                Picker("Provider", selection: Binding(get: { search.provider }, set: { configure(["provider": $0]) })) {
                    ForEach(search.providers) { provider in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack {
                                Text(provider.name)
                                if !provider.needsApiKey && !provider.needsBaseUrl { Tag("no key") }
                            }
                            Text(provider.note).font(.caption).foregroundStyle(.secondary)
                        }
                        .tag(provider.id)
                    }
                }
                .pickerStyle(.radioGroup)
                .labelsHidden()
            } header: {
                Text("Search Provider")
            } footer: {
                Text("Turn search on per message with the Web switch beside the message box. Kuro searches before the model answers, so it works even on models too small to ask for a tool.")
            }

            if search.needsApiKey || search.needsBaseUrl {
                Section {
                    if search.needsApiKey {
                        HStack {
                            SecureField("API key", text: $key, prompt: Text(search.hasApiKey ? "A key is stored — paste a new one to replace it" : "Paste the key"))
                            Button("Save") { configure(["apiKey": key]); key = "" }
                                .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
                            if search.hasApiKey { Button("Remove") { configure(["apiKey": ""]) } }
                        }
                    }
                    if search.needsBaseUrl {
                        HStack {
                            TextField("Instance URL", text: $baseUrl, prompt: Text(search.baseUrl ?? "http://localhost:8888"))
                            Button("Save") { configure(["baseUrl": baseUrl]); baseUrl = "" }
                                .disabled(baseUrl.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                    }
                    if let link = selected?.credentialsUrl.flatMap(URL.init(string:)) {
                        Link(search.needsApiKey ? "Get a key" : "How to run one", destination: link)
                    }
                } header: {
                    Text(selected?.name ?? "Settings")
                }
            }

            Section {
                HStack {
                    Button(testing ? "Searching…" : "Test Search", action: test).disabled(testing)
                    Text("Runs a real query, so you never have to guess.").foregroundStyle(.secondary)
                }
                if let failure { ErrorText(failure) }
                if let result {
                    if result.ok {
                        Label("Working.", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                        ForEach(result.results ?? [], id: \.url) { hit in
                            VStack(alignment: .leading, spacing: 1) {
                                Text(hit.title)
                                Text(hit.url).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                    } else {
                        ErrorText("Not working. \(result.error ?? "")")
                    }
                }
            }
        }
        .formStyle(.grouped)
    }

    private func configure(_ patch: KuroClient.Body) {
        perform($failure) {
            try await app.client.configureSearch(patch)
            result = nil
            app.invalidate(.tools)
        }
    }

    private func test() {
        testing = true
        perform($failure) {
            defer { testing = false }
            result = try await app.client.testSearch()
        }
    }
}

// MARK: - Memory

private struct MemoryPane: View {
    @Environment(AppModel.self) private var app
    let overview: ToolsOverview

    @State private var memories: [MemoryRecord]?
    @State private var failure: String?

    var body: some View {
        Form {
            Section {
                Toggle(isOn: Binding(get: { overview.memory.preload }, set: { on in
                    perform($failure) {
                        try await app.client.setMemoryPreload(on)
                        app.invalidate(.tools)
                    }
                })) {
                    Text("Put memories in front of the model automatically")
                    Text("Without this, memory only works when the model thinks to look — which small models often do not.")
                }
                LabeledContent("Stored", value: "\(overview.memory.count) \(overview.memory.count == 1 ? "memory" : "memories")")
            } footer: {
                Text("Durable facts a model was asked to keep. Stored on this Mac, available in every conversation, never sent anywhere. What you want every model to know about you lives in Settings → Chat.")
            }

            Section("Remembered") {
                if let failure { ErrorText(failure) }
                if let memories {
                    if memories.isEmpty { Text("Nothing yet. Ask the model to remember something.").foregroundStyle(.secondary) }
                    ForEach(memories) { memory in
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(memory.content).textSelection(.enabled)
                                if !memory.tags.isEmpty {
                                    HStack(spacing: 4) { ForEach(memory.tags, id: \.self) { Tag($0) } }
                                }
                            }
                            Spacer()
                            Button(role: .destructive) { forget(memory) } label: { Image(systemName: "trash") }
                                .buttonStyle(.borderless)
                                .help("Forget this")
                        }
                    }
                } else {
                    ProgressView()
                }
            }
        }
        .formStyle(.grouped)
        .task(id: app.stamp(.tools)) { memories = (try? await app.client.memories()) ?? memories ?? [] }
    }

    private func forget(_ memory: MemoryRecord) {
        perform($failure) {
            try await app.client.deleteMemory(memory.id)
            app.invalidate(.tools)
        }
    }
}

// MARK: - Built in

private struct BuiltinPane: View {
    let overview: ToolsOverview

    var body: some View {
        Form {
            Section {
                ForEach(overview.builtins) { tool in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(tool.name).font(.body.monospaced())
                            Tag(tool.group)
                        }
                        Text(tool.description).font(.callout).foregroundStyle(.secondary)
                    }
                }
            } header: {
                Text("What the Model Sees")
            } footer: {
                Text("Kuro's own tools, offered when their switch is on for a turn. Coding workspaces add their own file and command tools, depending on the mode.")
            }
        }
        .formStyle(.grouped)
    }
}
