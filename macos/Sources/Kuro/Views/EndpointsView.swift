import SwiftUI
import KuroKit

/// Providers and Cloud share one mechanism — store a key, probe an
/// OpenAI-compatible endpoint, list its models — but are different decisions:
/// a provider rents you their model per token; a cloud endpoint runs your model
/// on hardware you rent.
enum EndpointSurface: String {
    case provider, cloud

    var title: String { self == .provider ? "Providers" : "Cloud" }

    var intro: String {
        switch self {
        case .provider:
            "Talk to a model you do not run yourself. Your account, your key, your bill — requests go straight from this Mac to the provider."
        case .cloud:
            "Run your own models on hardware you rent or own: a GPU box, a server on your network, anything speaking the OpenAI API. Your instance, your endpoint, your bill."
        }
    }

    var urlPlaceholder: String {
        self == .provider ? "https://api.example.com/v1" : "https://your-pod-8000.proxy.runpod.net/v1"
    }

    var notes: [String] {
        switch self {
        case .provider:
            [
                "Provider models appear in the same picker as local ones, marked as leaving this Mac. Local models stay at the top.",
                "Everything else works the same: conversations, effort, web search, MCP tools, the details inspector.",
                "Keys are written to an owner-only file beside the database, never into it, and are never shown again once saved.",
                "Kuro speaks the OpenAI API. Anthropic is reached through its compatibility endpoint.",
                "To run your own weights on a rented GPU instead of paying per token, use Cloud.",
            ]
        case .cloud:
            [
                "Kuro does not run the machine for you. Start the server on your instance first, then give Kuro its address.",
                "The base URL is the part before /chat/completions, usually ending in /v1. Kuro asks for the model list on connect, so a wrong URL fails here rather than mid-conversation.",
                "Some engines need no key; put any value in the key field for those.",
                "Cloud models appear in the same picker as local ones and are marked as leaving this Mac.",
                "An endpoint on a private network works if this Mac can reach it.",
            ]
        }
    }

    static let kindLabels = [
        "aggregator": "Many Models, One Key",
        "first_party": "Direct from the Model Developer",
        "rented_gpu": "Hardware You Rent",
        "custom": "Anything OpenAI-Compatible",
    ]
}

struct EndpointsView: View {
    @Environment(AppModel.self) private var app
    let surface: EndpointSurface
    @State private var list: ProviderList?
    @State private var failure: String?

    var body: some View {
        let connected = list?.providers.filter { $0.surface == surface.rawValue } ?? []
        let presets = list?.presets.filter { $0.surface == surface.rawValue } ?? []

        Form {
            Section {
                Text(surface.intro).foregroundStyle(.secondary)
                if let failure { ErrorText(failure) }
            }

            if !connected.isEmpty {
                Section("Connected") {
                    ForEach(connected) { EndpointRow(provider: $0) }
                }
            }

            ForEach(kinds(presets), id: \.self) { kind in
                Section(EndpointSurface.kindLabels[kind] ?? kind.capitalized) {
                    ForEach(presets.filter { $0.kind == kind }) { preset in
                        PresetRow(
                            preset: preset,
                            connected: connected.contains { $0.provider == preset.slug },
                            urlPlaceholder: surface.urlPlaceholder
                        )
                    }
                }
            }

            Section("How This Fits") {
                ForEach(surface.notes, id: \.self) { note in
                    Label(note, systemImage: "circle.fill")
                        .labelStyle(BulletLabelStyle())
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(surface.title)
        .task(id: app.stamp(.providers)) {
            do {
                list = try await app.client.providers()
                failure = nil
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    private func kinds(_ presets: [ProviderPreset]) -> [String] {
        var seen: [String] = []
        for preset in presets where !seen.contains(preset.kind) { seen.append(preset.kind) }
        return seen
    }
}

private struct BulletLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            configuration.icon.font(.system(size: 4)).offset(y: -3)
            configuration.title
        }
    }
}

private struct PresetRow: View {
    @Environment(AppModel.self) private var app
    let preset: ProviderPreset
    let connected: Bool
    let urlPlaceholder: String

    @State private var open = false
    @State private var key = ""
    @State private var baseUrl = ""
    @State private var label = ""
    @State private var checking = false
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                ProviderLogo(slug: preset.slug, label: preset.name, size: 28)
                Text(preset.name).fontWeight(.medium)
                if connected { Tag("connected", tone: .live) }
                Spacer()
                if let link = preset.credentialsUrl.flatMap(URL.init(string:)) {
                    Link("Get a key", destination: link).font(.caption)
                }
                if !open {
                    Button(connected ? "Add Another…" : "Connect…") { open = true }
                }
            }
            Text(preset.blurb).font(.callout).foregroundStyle(.secondary)

            if open {
                VStack(alignment: .leading, spacing: 8) {
                    if preset.needsUrl {
                        TextField("Base URL", text: $baseUrl, prompt: Text(urlPlaceholder)).font(.body.monospaced())
                    }
                    SecureField("API key", text: $key, prompt: Text(preset.keyHint ?? "Paste the key"))
                    if connected {
                        TextField("Name", text: $label, prompt: Text("\(preset.name) (second account)"))
                    }
                    if let failure { ErrorText(failure) }
                    HStack {
                        Spacer()
                        Button("Cancel") { open = false }
                        Button(checking ? "Checking…" : "Connect", action: connect)
                            .buttonStyle(.borderedProminent)
                            .disabled(!canSubmit || checking)
                    }
                }
                .padding(.top, 4)
            }
        }
        .padding(.vertical, 2)
    }

    private var canSubmit: Bool {
        !key.trimmingCharacters(in: .whitespaces).isEmpty && (!preset.needsUrl || !baseUrl.trimmingCharacters(in: .whitespaces).isEmpty)
    }

    private func connect() {
        var body: KuroClient.Body = ["provider": preset.slug, "apiKey": key]
        if preset.needsUrl { body["baseUrl"] = baseUrl.trimmingCharacters(in: .whitespaces) }
        if !label.trimmingCharacters(in: .whitespaces).isEmpty { body["label"] = label.trimmingCharacters(in: .whitespaces) }
        checking = true
        perform($failure) {
            defer { checking = false }
            let added = try await app.client.addProvider(body)
            app.invalidate(.providers, .models)
            // Saved, but the endpoint did not answer: keep the form open with the reason.
            if added.status == "error" {
                failure = added.lastError ?? "The endpoint did not answer."
                return
            }
            key = ""
            baseUrl = ""
            label = ""
            open = false
        }
    }
}

private struct EndpointRow: View {
    @Environment(AppModel.self) private var app
    let provider: Provider

    @State private var showModels = false
    @State private var editingKey = false
    @State private var key = ""
    @State private var busy = false
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                ProviderLogo(slug: provider.provider, label: provider.label, size: 24)
                StatusDot(color: StatusDot.color(for: provider.status))
                Text(provider.label).fontWeight(.medium)
                if !provider.models.isEmpty {
                    Button("\(provider.models.count) models") { withAnimation(.snappy) { showModels.toggle() } }
                        .buttonStyle(.link)
                        .font(.caption)
                }
                if !provider.hasKey { Tag("no key", tone: .warning) }
                if let tested = provider.lastTestedAt {
                    Text("checked \(Naming.relative(tested))").font(.caption).foregroundStyle(.tertiary)
                }
                Spacer()
                actions
            }
            Text(provider.baseUrl).font(.caption.monospaced()).foregroundStyle(.secondary)
            if provider.status == "error", let error = provider.lastError { ErrorText(error) }
            if let failure { ErrorText(failure) }
            if showModels {
                Text(provider.models.joined(separator: "  ·  "))
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            if editingKey {
                HStack {
                    SecureField("New API key", text: $key)
                    Button("Save and Check") {
                        let replacement = key
                        act { try await app.client.replaceProviderKey(provider.id, replacement) }
                        key = ""
                        editingKey = false
                    }
                    .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
                    Button("Cancel") { editingKey = false }
                }
            }
        }
        .padding(.vertical, 2)
        .opacity(provider.enabled ? 1 : 0.6)
    }

    private var actions: some View {
        HStack(spacing: 6) {
            Button { act { try await app.client.testProvider(provider.id) } } label: {
                if busy { ProgressView().controlSize(.mini) } else { Image(systemName: "arrow.clockwise") }
            }
            .help("Check and refresh the model list")
            Button { editingKey.toggle() } label: { Image(systemName: "key") }
                .help("Replace the key")
            Toggle("Enabled", isOn: Binding(get: { provider.enabled }, set: { on in act { try await app.client.setProviderEnabled(provider.id, on) } }))
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.mini)
            Button(role: .destructive) { act { try await app.client.deleteProvider(provider.id) } } label: { Image(systemName: "trash") }
                .help("Remove, and delete its key")
        }
        .buttonStyle(.borderless)
    }

    private func act(_ work: @escaping () async throws -> Void) {
        busy = true
        perform($failure) {
            defer { busy = false }
            try await work()
            app.invalidate(.providers, .models)
        }
    }
}
