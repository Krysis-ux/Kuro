import AppKit
import SwiftUI
import KuroKit

/// Installed models, downloads, and finding new ones.
struct ModelsView: View {
    @Environment(AppModel.self) private var app

    enum Source: String, CaseIterable { case recommended = "Recommended", hub = "Hugging Face" }

    @State private var reference = ""
    @State private var pulling = false
    @State private var failure: String?
    @State private var downloads: [DownloadRecord] = []
    @State private var recommended: RecommendedList?
    @State private var source: Source = .recommended
    @State private var deleting: InstalledModel?

    var body: some View {
        Form {
            Section {
                HStack {
                    TextField("Model", text: $reference, prompt: Text("owner/repository or a Hugging Face URL"))
                        .labelsHidden()
                        .onSubmit { pull(reference) }
                    Button(pulling ? "Starting…" : "Download") { pull(reference) }
                        .disabled(reference.trimmingCharacters(in: .whitespaces).isEmpty || pulling)
                }
                if let failure { ErrorText(failure) }
            } header: {
                Text("Add by Name")
            } footer: {
                Text("Add `:Q5_K_M` to choose a quantization. Only GGUF weights can run here. Weights are stored on this Mac and never leave it.")
            }

            let active = downloads.filter(\.isActive)
            if !active.isEmpty {
                Section("Downloading") {
                    ForEach(active) { download in
                        DownloadRow(download: download) { cancel(download) }
                    }
                }
            }

            Section("Installed") {
                if app.models.models.isEmpty {
                    Text("Nothing installed yet. Pick one below.").foregroundStyle(.secondary)
                }
                ForEach(app.models.models) { entry in installedRow(entry) }
            }

            Section {
                Picker("Source", selection: $source) {
                    ForEach(Source.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            } header: {
                Text("Discover")
            } footer: {
                Text(source == .recommended
                     ? "Grouped by what each is for. Fit is estimated from this Mac's memory and the model's size — a guide, not a benchmark."
                     : "Only repositories publishing GGUF weights, most downloaded first.")
            }

            switch source {
            case .recommended: recommendedSections
            case .hub: HubSearch(onPull: pull)
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Models")
        .toolbar { BackToolbarItem() }
        .task(id: app.stamp(.models)) {
            recommended = (try? await app.client.recommended()) ?? recommended
        }
        .task { await pollDownloads(app) { downloads = $0 } }
        .confirmationDialog("Delete \(deleting?.model.id ?? "")?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete", role: .destructive) {
                guard let deleting else { return }
                perform($failure) {
                    try await app.client.deleteModel(deleting.model.id)
                    app.invalidate(.models)
                }
            }
        } message: {
            Text("This frees \(Naming.bytes(deleting?.model.fileSizeBytes)). Getting it back means downloading it again.")
        }
    }

    private func installedRow(_ entry: InstalledModel) -> some View {
        let owner = Naming.publisher(of: entry.model.id)
        return HStack(spacing: 10) {
            OpensModel(repo: entry.model.hfRepo) {
                HStack(spacing: 10) {
                    ModelLogo(name: entry.model.hfRepo ?? entry.model.id, owner: owner, size: 34)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(entry.model.displayName.isEmpty ? entry.model.id : entry.model.displayName)
                            .fontWeight(.medium).lineLimit(1).truncationMode(.middle)
                        HStack(spacing: 4) {
                            if let quant = entry.model.quant { Tag(quant.uppercased()) }
                            ForEach(entry.model.capabilities, id: \.self) { Tag($0) }
                            if entry.loaded { Tag("loaded", tone: .live) }
                            if entry.model.status == "error" { Tag("error", tone: .error) }
                            if let owner { Text("via \(owner)").font(.caption).foregroundStyle(.tertiary) }
                        }
                        if let error = entry.model.error { ErrorText(error) }
                    }
                }
            }
            Spacer()
            Text(Naming.bytes(entry.model.fileSizeBytes)).font(.caption.monospaced()).foregroundStyle(.secondary)
            if let fit = entry.fit { FitBadge(fit: fit) }
            Menu {
                if entry.loaded {
                    Button("Unload from Memory") { act { try await app.client.unloadModel(entry.model.id) } }
                } else if entry.model.status == "ready" {
                    Button("Load Now") { act { try await app.client.loadModel(entry.model.id) } }
                }
                if let repo = entry.model.hfRepo {
                    Button("About This Model") { app.go(.model(repo), returning: true) }
                }
                Divider()
                Button("Delete…", role: .destructive) { deleting = entry }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
        }
    }

    @ViewBuilder private var recommendedSections: some View {
        if let recommended {
            ForEach(recommended.purposes) { purpose in
                let models = recommended.models.filter { $0.purposes.contains(purpose.id) }
                if !models.isEmpty {
                    Section {
                        ForEach(models) { model in
                            HStack(alignment: .top, spacing: 12) {
                                OpensModel(repo: model.repo) {
                                    HStack(alignment: .top, spacing: 12) {
                                        ModelLogo(name: model.repo, size: 34)
                                        VStack(alignment: .leading, spacing: 4) {
                                            HStack {
                                                Text(model.displayName).fontWeight(.medium)
                                                FitBadge(fit: model.fit)
                                            }
                                            Text(model.blurb).font(.callout).foregroundStyle(.secondary)
                                            HStack(spacing: 4) {
                                                Tag(model.paramCount)
                                                Tag(model.defaultQuant)
                                                Tag(Naming.bytes(model.approxSizeBytes))
                                                ForEach(model.capabilities, id: \.self) { Tag($0) }
                                            }
                                        }
                                    }
                                }
                                Spacer()
                                Button(model.installed ? "Installed" : "Download") { pull(model.slug) }
                                    .disabled(model.installed || pulling)
                            }
                            .padding(.vertical, 2)
                        }
                    } header: {
                        Text(purpose.label)
                    } footer: {
                        Text(purpose.blurb)
                    }
                }
            }
        } else {
            Section { ProgressView() }
        }
    }

    // MARK: Actions

    private func pull(_ model: String) {
        let trimmed = model.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        pulling = true
        perform($failure) {
            defer { pulling = false }
            try await app.client.pull(trimmed)
            reference = ""
            app.invalidate(.models)
            downloads = (try? await app.client.downloads()) ?? downloads
        }
    }

    private func cancel(_ download: DownloadRecord) {
        act { try await app.client.cancelDownload(download.id) }
    }

    private func act(_ work: @escaping () async throws -> Void) {
        perform($failure) {
            try await work()
            app.invalidate(.models)
        }
    }
}

/// Fast while something is downloading, slow otherwise. A finished download
/// changes what is installed, so the model list reloads when one ends.
@MainActor
func pollDownloads(_ app: AppModel, update: ([DownloadRecord]) -> Void) async {
    var wasActive = false
    while !Task.isCancelled {
        let list = (try? await app.client.downloads()) ?? []
        update(list)
        let active = list.contains(where: \.isActive)
        if wasActive && !active { app.invalidate(.models) }
        wasActive = active
        try? await Task.sleep(for: active ? .milliseconds(700) : .seconds(4))
    }
}

/// One download in flight, with its model's logo and a way to stop it.
struct DownloadRow: View {
    let download: DownloadRecord
    let onCancel: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            ModelLogo(name: download.label, size: 28)
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(download.label).lineLimit(1)
                    Spacer()
                    Text("\(Naming.bytes(download.downloadedBytes)) of \(Naming.bytes(download.totalBytes))")
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                    Button(action: onCancel) { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.secondary)
                        .help("Cancel this download")
                }
                ProgressView(value: download.fraction)
            }
        }
    }
}

/// Makes its content open a model's detail screen, when it has a repository to show.
private struct OpensModel<Content: View>: View {
    @Environment(AppModel.self) private var app
    let repo: String?
    @ViewBuilder let content: Content

    var body: some View {
        if let repo {
            Button { app.go(.model(repo), returning: true) } label: { content.contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .help("About this model")
        } else {
            content
        }
    }
}

/// Search Hugging Face for GGUF repositories.
private struct HubSearch: View {
    @Environment(AppModel.self) private var app
    let onPull: (String) -> Void

    @State private var query = ""
    @State private var results: [HubModel]?
    @State private var searching = false
    @State private var failure: String?

    var body: some View {
        Section {
            HStack {
                TextField("Search", text: $query, prompt: Text("Search Hugging Face…"))
                    .labelsHidden()
                    .onSubmit(search)
                Button(searching ? "Searching…" : "Search", action: search).disabled(searching)
                Link(destination: URL(string: "https://huggingface.co/models?library=gguf&sort=downloads")!) {
                    Image(systemName: "arrow.up.right.square")
                }
                .help("Browse on Hugging Face")
            }
            if let failure { ErrorText(failure) }
            if let results, results.isEmpty { Text("Nothing matched.").foregroundStyle(.secondary) }
            ForEach(results ?? []) { model in HubRow(model: model, onPull: onPull) }
        }
        .task { if results == nil { search() } }
    }

    private func search() {
        searching = true
        perform($failure) {
            defer { searching = false }
            results = try await app.client.searchHub(query)
        }
    }
}

private struct HubRow: View {
    let model: HubModel
    let onPull: (String) -> Void
    @State private var quant: String?

    var body: some View {
        // The smallest quantization is listed first and is the likeliest to fit.
        let chosen = quant ?? model.quants.first
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                OpensModel(repo: model.repo) {
                    HStack(spacing: 10) {
                        ModelLogo(name: model.name, owner: model.owner, size: 34)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(model.name).fontWeight(.medium).lineLimit(1)
                            if !model.owner.isEmpty {
                                HStack(spacing: 4) {
                                    HubAvatar(owner: model.owner, size: 14)
                                    Text(model.owner)
                                }
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                            }
                        }
                    }
                }
                if let params = model.paramCount { Tag(params) }
                if model.installed { Tag("installed", tone: .live, systemImage: "checkmark") }
                if model.gated { Tag("licence needed", tone: .warning) }
                if model.splitOnly { Tag("split shards", tone: .warning) }
                Spacer()
                if let fit = model.fit { FitBadge(fit: fit) }
            }
            HStack(spacing: 10) {
                Text("\(Naming.count(model.downloads)) downloads")
                if model.likes > 0 { Text("\(Naming.count(model.likes)) likes") }
                if let modified = model.lastModified { Text("updated \(Naming.relative(modified))") }
                Link("Repository", destination: URL(string: "https://huggingface.co/\(model.repo)")!)
                Spacer()
                if !model.quants.isEmpty {
                    Picker("Quantization", selection: Binding(get: { chosen ?? "" }, set: { quant = $0 })) {
                        ForEach(model.quants, id: \.self) { Text($0).tag($0) }
                    }
                    .labelsHidden()
                    .fixedSize()
                }
                Button("Download") { onPull(chosen.map { "\(model.repo):\($0)" } ?? model.repo) }
                    .disabled(model.splitOnly || model.gated)
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            if model.splitOnly {
                Text("Published only as multi-part shards, which Kuro cannot load yet. Look for a smaller quantization in a single file.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if model.gated {
                Text("Gated — accept its licence on Hugging Face first, then download it by name above.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }
}
