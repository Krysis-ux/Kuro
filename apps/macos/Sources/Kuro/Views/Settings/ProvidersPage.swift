import SwiftUI
import KuroKit

/// Every model reached over the internet, in one place: Kuro Free, the providers you hold a key
/// for, and endpoints on hardware you rent.
struct ProvidersPage: View {
    @Environment(AppModel.self) private var app

    enum Selection: Hashable {
        case free
        case provider(String)
        case add
        case preset(String)
    }

    @State private var list: ProviderList?
    @State private var free: FreeOverview?
    @State private var selection: Selection?
    @State private var failure: String?

    var body: some View {
        SettingsPage(pane: .providers, width: 980) {
            HStack(spacing: 8) {
                Button { app.invalidate(.providers, .free, .models) } label: { Image(systemName: "arrow.clockwise") }
                    .buttonStyle(SubtleButtonStyle())
                    .help("Refresh")
                Button { selection = .add } label: { Label("Add provider", systemImage: "plus") }
                    .buttonStyle(PrimaryButtonStyle())
            }
        } content: {
            HStack(spacing: 0) {
                providerList
                    .frame(width: 230)
                Rectangle().fill(Palette.line).frame(width: 1)
                detail
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .card(cornerRadius: 14)
            .padding(20)
            .frame(maxWidth: 980)
            .frame(maxWidth: .infinity)
        }
        .task(id: [app.stamp(.providers), app.stamp(.free)]) { await load() }
        #if DEBUG
        .onReceive(NotificationCenter.default.publisher(for: .init("kuro.debug.providers"))) { note in
            switch note.object as? String {
            case "add": selection = .add
            case "free": selection = .free
            case let slug?: selection = .preset(slug)
            default: break
            }
        }
        #endif
    }

    private var connected: [Provider] { list?.providers ?? [] }
    private var current: Selection { selection ?? defaultSelection }

    private var defaultSelection: Selection {
        if let first = connected.first { return .provider(first.id) }
        return (free?.keyCount ?? 0) > 0 ? .free : .add
    }

    // MARK: List

    private var providerList: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 1) {
                SidebarHeading("Pools")
                SidebarRow(title: "Kuro Free", systemImage: "gift", selected: current == .free, action: { selection = .free }) {
                    StatusDot(color: (free?.availableCount ?? 0) > 0 ? .green : .secondary.opacity(0.4))
                }

                group("Providers", connected.filter { $0.surface == "provider" })
                group("Your Hardware", connected.filter { $0.surface == "cloud" })

                if connected.isEmpty {
                    SidebarHeading("Providers")
                    Text("None connected yet.")
                        .font(.callout)
                        .foregroundStyle(.tertiary)
                        .padding(.horizontal, 8)
                }
                if let failure { ErrorText(failure).padding(8) }
            }
            .padding(10)
        }
    }

    @ViewBuilder private func group(_ title: String, _ providers: [Provider]) -> some View {
        if !providers.isEmpty {
            SidebarHeading(title)
            ForEach(providers) { provider in
                Button { selection = .provider(provider.id) } label: {
                    HStack(spacing: 9) {
                        ProviderLogo(slug: provider.provider, label: provider.label, size: 20, bare: true)
                        Text(provider.label).lineLimit(1)
                        Spacer(minLength: 4)
                        StatusDot(color: provider.enabled ? StatusDot.color(for: provider.status) : .secondary.opacity(0.3))
                    }
                    .font(.system(size: 13))
                    .padding(.horizontal, 8)
                    .frame(height: 30)
                    .background(current == .provider(provider.id) ? Palette.selected : .clear, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    // MARK: Detail

    @ViewBuilder private var detail: some View {
        switch current {
        case .free:
            FreeDetail()
        case .provider(let id):
            if let provider = connected.first(where: { $0.id == id }) {
                ProviderDetail(provider: provider) { selection = nil }
            } else {
                EmptyNote(title: "Not found", systemImage: "questionmark")
            }
        case .add:
            AddProviderGrid(presets: list?.presets ?? [], connected: connected) { selection = .preset($0.slug) }
        case .preset(let slug):
            if let preset = list?.presets.first(where: { $0.slug == slug }) {
                ConnectForm(
                    preset: preset,
                    alreadyConnected: connected.contains { $0.provider == slug },
                    onBack: { selection = .add },
                    onDone: { selection = .provider($0) }
                )
                .id(slug)
            }
        }
    }

    private func load() async {
        do {
            async let providers = app.client.providers()
            async let pool = app.client.free()
            (list, free) = try await (providers, pool)
            failure = nil
        } catch {
            failure = error.localizedDescription
        }
    }
}

// MARK: - One provider

private struct ProviderDetail: View {
    @Environment(AppModel.self) private var app
    let provider: Provider
    let onRemoved: () -> Void

    @State private var editingKey = false
    @State private var key = ""
    @State private var busy = false
    @State private var confirmRemove = false
    @State private var modelQuery = ""
    @State private var failure: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack(spacing: 12) {
                    ProviderLogo(slug: provider.provider, label: provider.label, size: 36)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(provider.label).font(.system(size: 16, weight: .semibold))
                        Text(provider.baseUrl)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .textSelection(.enabled)
                    }
                    Spacer()
                    Toggle("Enabled", isOn: Binding(get: { provider.enabled }, set: { on in act { try await app.client.setProviderEnabled(provider.id, on) } }))
                        .toggleStyle(.switch)
                        .labelsHidden()
                        .help(provider.enabled ? "Shown in the model picker" : "Hidden from the model picker")
                }

                HStack(spacing: 6) {
                    StatusDot(color: StatusDot.color(for: provider.status))
                    Text(statusLine).font(.callout).foregroundStyle(.secondary)
                    if !provider.hasKey { Tag("no key", tone: .warning) }
                }
                if provider.status == "error", let error = provider.lastError { ErrorText(error) }
                if let failure { ErrorText(failure) }

                HStack(spacing: 8) {
                    Button { act { try await app.client.testProvider(provider.id) } } label: {
                        Label(busy ? "Checking…" : "Check", systemImage: "arrow.clockwise")
                    }
                    .disabled(busy)
                    Button { editingKey.toggle() } label: { Label("Replace key", systemImage: "key") }
                    Spacer()
                    Button(role: .destructive) { confirmRemove = true } label: { Label("Remove", systemImage: "trash") }
                }
                .buttonStyle(SubtleButtonStyle())

                if editingKey {
                    HStack {
                        SecureField("New API key", text: $key)
                            .textFieldStyle(.roundedBorder)
                        Button("Save") {
                            let replacement = key
                            act { try await app.client.replaceProviderKey(provider.id, replacement) }
                            key = ""
                            editingKey = false
                        }
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }

                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 6) {
                        Text("Models").font(.system(size: 13, weight: .medium))
                        Text("\(provider.models.count)").foregroundStyle(.tertiary)
                        Spacer()
                        if provider.models.count > 12 {
                            TextField("Filter", text: $modelQuery, prompt: Text("Filter models"))
                                .textFieldStyle(.roundedBorder)
                                .frame(width: 180)
                        }
                    }
                    let shown = modelQuery.isEmpty ? provider.models : provider.models.filter { $0.localizedCaseInsensitiveContains(modelQuery) }
                    if provider.models.isEmpty {
                        Text("None listed. Press Check to ask again.").font(.callout).foregroundStyle(.secondary)
                    } else if shown.isEmpty {
                        Text("Nothing matched.").font(.callout).foregroundStyle(.secondary)
                    } else {
                        LazyVStack(spacing: 0) {
                            ForEach(Array(shown.enumerated()), id: \.offset) { index, model in
                                if index > 0 { Rectangle().fill(Palette.line).frame(height: 1) }
                                HStack {
                                    Text(model).font(.callout.monospaced()).lineLimit(1).truncationMode(.middle)
                                    Spacer()
                                    CopyButton(text: model).labelStyle(.iconOnly).buttonStyle(.borderless).controlSize(.small)
                                }
                                .padding(.horizontal, 12)
                                .frame(height: 34)
                            }
                        }
                        .card(cornerRadius: 10, fill: Palette.sunken)
                    }
                }
            }
            .padding(20)
        }
        .opacity(provider.enabled ? 1 : 0.7)
        .confirmationDialog("Remove \(provider.label)?", isPresented: $confirmRemove) {
            Button("Remove", role: .destructive) {
                act {
                    try await app.client.deleteProvider(provider.id)
                    onRemoved()
                }
            }
        } message: {
            Text("Its key is deleted from this Mac.")
        }
    }

    private var statusLine: String {
        let state = switch provider.status {
        case "ok": "Connected"
        case "error": "Not answering"
        default: "Not checked yet"
        }
        guard let tested = provider.lastTestedAt else { return state }
        return "\(state) · checked \(Naming.relative(tested))"
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

// MARK: - Adding one

private struct AddProviderGrid: View {
    let presets: [ProviderPreset]
    let connected: [Provider]
    let onPick: (ProviderPreset) -> Void

    private static let groups: [(kind: String, title: String, help: String)] = [
        ("aggregator", "Many Models, One Key", "One account reaches models from many makers."),
        ("first_party", "Model Makers", "Straight from the company that trained the model."),
        ("rented_gpu", "Your Hardware", "Run your own models on a GPU you rent. Start the server there first, then give Kuro its address."),
        ("custom", "Anything Else", "Any endpoint that speaks the OpenAI API. The base URL usually ends in /v1."),
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Add provider").font(.system(size: 16, weight: .semibold))
                ForEach(Self.groups, id: \.kind) { group in
                    let members = presets.filter { $0.kind == group.kind }
                    if !members.isEmpty {
                        VStack(alignment: .leading, spacing: 8) {
                            TitleWithHelp(group.title, help: group.help)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 8)], spacing: 8) {
                                ForEach(members) { preset in
                                    tile(preset)
                                }
                            }
                        }
                    }
                }
                // Presets of a kind this version does not know still get listed.
                let known = Set(Self.groups.map(\.kind))
                let rest = presets.filter { !known.contains($0.kind) }
                if !rest.isEmpty {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 8)], spacing: 8) {
                        ForEach(rest) { tile($0) }
                    }
                }
            }
            .padding(20)
        }
    }

    private func tile(_ preset: ProviderPreset) -> some View {
        Button { onPick(preset) } label: {
            HStack(spacing: 10) {
                ProviderLogo(slug: preset.slug, label: preset.name, size: 28)
                Text(preset.name).lineLimit(1)
                Spacer(minLength: 4)
                if connected.contains(where: { $0.provider == preset.slug }) {
                    Image(systemName: "checkmark").foregroundStyle(.green).font(.caption)
                }
                Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
            }
            .font(.system(size: 13))
            .padding(.horizontal, 12)
            .frame(height: 50)
            .card(cornerRadius: 10, fill: Palette.sunken)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(preset.blurb)
    }
}

private struct ConnectForm: View {
    @Environment(AppModel.self) private var app
    let preset: ProviderPreset
    let alreadyConnected: Bool
    let onBack: () -> Void
    let onDone: (String) -> Void

    @State private var key = ""
    @State private var baseUrl = ""
    @State private var label = ""
    @State private var checking = false
    @State private var failure: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 10) {
                    Button(action: onBack) { Image(systemName: "arrow.left") }
                        .buttonStyle(.plain)
                        .help("Back")
                    ProviderLogo(slug: preset.slug, label: preset.name, size: 28)
                    Text(preset.name).font(.system(size: 16, weight: .semibold))
                    HelpTip(preset.blurb)
                }

                VStack(alignment: .leading, spacing: 10) {
                    if preset.needsUrl {
                        field("Base URL") {
                            TextField("Base URL", text: $baseUrl, prompt: Text(preset.surface == "cloud" ? "https://your-pod-8000.proxy.runpod.net/v1" : "https://api.example.com/v1"))
                                .font(.body.monospaced())
                        }
                    }
                    field("API key") {
                        SecureField("API key", text: $key, prompt: Text(preset.keyHint ?? "Paste the key"))
                            .onSubmit(connect)
                    }
                    if alreadyConnected {
                        field("Name") {
                            TextField("Name", text: $label, prompt: Text("\(preset.name) (second account)"))
                        }
                    }
                }
                .textFieldStyle(.roundedBorder)

                if let failure { ErrorText(failure) }

                HStack {
                    if let link = preset.credentialsUrl.flatMap(URL.init(string:)) {
                        Link(destination: link) { Label("Get a key", systemImage: "arrow.up.right") }
                            .font(.callout)
                    }
                    Spacer()
                    Button("Cancel", action: onBack).buttonStyle(SubtleButtonStyle())
                    Button(checking ? "Checking…" : "Connect", action: connect)
                        .buttonStyle(PrimaryButtonStyle())
                        .disabled(!canSubmit || checking)
                }
            }
            .padding(20)
            .frame(maxWidth: 540, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func field<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            content()
        }
    }

    private var canSubmit: Bool {
        !key.trimmingCharacters(in: .whitespaces).isEmpty && (!preset.needsUrl || !baseUrl.trimmingCharacters(in: .whitespaces).isEmpty)
    }

    private func connect() {
        guard canSubmit, !checking else { return }
        var body: KuroClient.Body = ["provider": preset.slug, "apiKey": key]
        if preset.needsUrl { body["baseUrl"] = baseUrl.trimmingCharacters(in: .whitespaces) }
        if !label.trimmingCharacters(in: .whitespaces).isEmpty { body["label"] = label.trimmingCharacters(in: .whitespaces) }
        checking = true
        perform($failure) {
            defer { checking = false }
            let added = try await app.client.addProvider(body)
            app.invalidate(.providers, .models)
            // Saved, but the endpoint did not answer: it is listed with the reason.
            onDone(added.id)
        }
    }
}
