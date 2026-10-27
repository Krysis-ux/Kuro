import AVKit
import AppKit
import SwiftUI
import KuroKit

/// One Hugging Face repository, with everything the Hub shows about it.
///
/// The question this screen answers first is "which file do I download", so
/// the quantizations lead, each with its real size and how it fits this Mac;
/// the model card, papers and specs follow. Works for repositories without GGUF
/// weights too, which is how a quantized model's original is shown.
struct ModelDetailView: View {
    @Environment(AppModel.self) private var app
    let repo: String

    @State private var card: HubModelCard?
    @State private var blocks: [CardBlock] = []
    @State private var failure: String?
    @State private var pullFailure: String?
    @State private var pulling = false
    @State private var downloads: [DownloadRecord] = []

    private var owner: String { String(repo.split(separator: "/").first ?? "") }
    private var name: String { repo.split(separator: "/").last.map(String.init) ?? repo }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                hero

                if let card {
                    if card.gated {
                        Label("Gated — accept its licence on Hugging Face before downloading.", systemImage: "lock")
                            .foregroundStyle(.orange)
                    }
                    if !card.baseModels.isEmpty { baseModels(card.baseModels) }

                    let active = downloads.filter(\.isActive)
                    if !active.isEmpty {
                        Panel(title: "Downloading", systemImage: "arrow.down.circle") {
                            ForEach(active) { download in
                                DownloadRow(download: download) {
                                    perform($pullFailure) { try await app.client.cancelDownload(download.id) }
                                }
                            }
                        }
                    }

                    ViewThatFits(in: .horizontal) {
                        HStack(alignment: .top, spacing: 16) {
                            modelCard(card).frame(minWidth: 460, idealWidth: 460, maxWidth: .infinity)
                            aside(card).frame(width: 340)
                        }
                        VStack(alignment: .leading, spacing: 16) {
                            aside(card)
                            modelCard(card)
                        }
                    }
                } else if let failure {
                    ErrorText(failure)
                } else {
                    ProgressView().frame(maxWidth: .infinity).padding(48)
                }
            }
            .padding(24)
            .frame(maxWidth: 1180)
            .frame(maxWidth: .infinity)
        }
        .navigationTitle(name)
        .toolbar {
            BackToolbarItem()
            ToolbarItem {
                Link(destination: Self.hub(repo)) {
                    Label("Open on Hugging Face", systemImage: "arrow.up.right.square")
                }
                .help("Open on Hugging Face")
            }
        }
        .task(id: app.stamp(.models)) { await load() }
        .task { await pollDownloads(app) { downloads = $0 } }
    }

    // MARK: Header

    private var hero: some View {
        HStack(spacing: 18) {
            ModelLogo(name: name, owner: owner, size: 64)
                .shadow(color: .black.opacity(0.15), radius: 8, y: 3)
            VStack(alignment: .leading, spacing: 6) {
                Link(destination: Self.hub(owner)) {
                    HStack(spacing: 6) {
                        HubAvatar(owner: owner, size: 18)
                        Text(owner)
                    }
                }
                .foregroundStyle(.secondary)
                Text(name)
                    .font(.system(size: 26, weight: .semibold))
                    .textSelection(.enabled)
                if let card {
                    HStack(spacing: 14) {
                        Label(Naming.count(card.downloads), systemImage: "arrow.down.circle")
                            .help("Downloads in the last month")
                        Label(Naming.count(card.likes), systemImage: "heart")
                        if let modified = card.lastModified { Text("Updated \(Naming.relative(modified))") }
                        if let task = card.pipelineTag { Tag(task.replacingOccurrences(of: "-", with: " ")) }
                        if let license = card.license { Tag(license) }
                    }
                    .font(.callout)
                    .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(22)
        .background {
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(RadialGradient(colors: [Color.primary.opacity(0.08), Color.primary.opacity(0.025)],
                                     center: .topLeading, startRadius: 0, endRadius: 520))
        }
        .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Color.primary.opacity(0.09)))
    }

    private func baseModels(_ repos: [String]) -> some View {
        HStack(spacing: 8) {
            Text("Built from").foregroundStyle(.secondary)
            ForEach(repos, id: \.self) { base in
                Button {
                    app.go(.model(base), returning: true)
                } label: {
                    HStack(spacing: 6) {
                        ModelLogo(name: base, owner: base.split(separator: "/").first.map(String.init), size: 20)
                        Text(base)
                        Image(systemName: "arrow.right").font(.caption).foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                    .padding(.leading, 4)
                    .padding(.trailing, 10)
                    .background(Color.primary.opacity(0.05), in: Capsule())
                    .overlay(Capsule().strokeBorder(Color.primary.opacity(0.1)))
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .help("Open the model this was built from")
            }
        }
    }

    // MARK: Columns

    private func modelCard(_ card: HubModelCard) -> some View {
        Panel(title: "Model Card", systemImage: "doc.text") {
            if blocks.isEmpty {
                Text("This repository has no model card.").foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                        switch block {
                        case .markdown(let text):
                            MarkdownView(text: text)
                        case .images(let images):
                            FlowLayout(spacing: 6) {
                                ForEach(images, id: \.url) { CardImageView(image: $0, inRow: images.count > 1) }
                            }
                            .frame(maxWidth: .infinity, alignment: images.count > 1 ? .leading : .center)
                        case .video(let url):
                            CardVideo(url: url)
                        }
                    }
                }
            }
        }
    }

    private func aside(_ card: HubModelCard) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            files(card)
            specs(card)
            if !card.papers.isEmpty { papers(card.papers) }
            extras(card)
        }
    }

    private func files(_ card: HubModelCard) -> some View {
        let best = card.bestFit
        return Panel(title: "Quantizations", systemImage: "square.stack.3d.down.right", trailing: card.files.isEmpty ? nil : "\(card.files.count) files") {
            if card.files.isEmpty {
                Text(card.baseModels.isEmpty
                     ? "No single-file GGUF weights here — this is the original model. Search for a GGUF build of it to run it in Kuro."
                     : "No single-file GGUF weights here.")
                    .foregroundStyle(.secondary)
            }
            VStack(spacing: 2) {
                ForEach(card.files) { file in
                    HStack(spacing: 8) {
                        Text(file.quant ?? "GGUF")
                            .font(.caption.monospaced())
                            .padding(.horizontal, 5)
                            .padding(.vertical, 2)
                            .background(Color.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 4))
                        if let bits = file.bits { Text(bits).font(.caption).foregroundStyle(.secondary) }
                        if file == best { Tag("best fit", tone: .live) }
                        Spacer(minLength: 4)
                        Text(Naming.bytes(file.size)).font(.caption.monospaced()).foregroundStyle(.secondary)
                        FitBadge(fit: file.fit)
                        Button { pull(file) } label: {
                            Image(systemName: file.installed ? "checkmark.circle.fill" : "arrow.down.circle")
                        }
                        .buttonStyle(.borderless)
                        .disabled(file.installed || pulling || card.gated || file.quant == nil)
                        .help(file.installed ? "Installed" : "Download \(Naming.bytes(file.size))")
                    }
                    .padding(.vertical, 4)
                    .padding(.horizontal, 6)
                    .background(file == best ? Color.accentColor.opacity(0.1) : .clear, in: RoundedRectangle(cornerRadius: 7))
                    .help(file.filename)
                }
            }
            if !card.splitQuants.isEmpty {
                Text("Also published as split shards, which Kuro cannot load yet: \(card.splitQuants.joined(separator: ", ")).")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if card.hasProjector {
                Text("Ships a vision projector (mmproj) for image input.").font(.caption).foregroundStyle(.secondary)
            }
            if let pullFailure { ErrorText(pullFailure) }
        }
    }

    private func specs(_ card: HubModelCard) -> some View {
        let rows: [(String, String?)] = [
            ("Parameters", card.gguf?.total.map(Self.params)),
            ("Architecture", card.gguf?.architecture),
            ("Context", card.gguf?.contextLength.map { "\(Naming.count($0)) tokens" }),
            ("Task", card.pipelineTag?.replacingOccurrences(of: "-", with: " ")),
            ("Library", card.libraryName),
            ("License", card.license),
            ("Languages", card.languages.isEmpty ? nil : card.languages.prefix(8).joined(separator: ", ")),
            ("Published", card.createdAt.flatMap(Naming.date).map { $0.formatted(date: .abbreviated, time: .omitted) }),
        ]
        let shown = rows.compactMap { label, value in value.map { (label, $0) } }

        return Panel(title: "At a Glance", systemImage: "info.circle") {
            Grid(alignment: .leading, verticalSpacing: 8) {
                ForEach(shown, id: \.0) { label, value in
                    GridRow {
                        Text(label).foregroundStyle(.secondary)
                        Text(value).frame(maxWidth: .infinity, alignment: .trailing).textSelection(.enabled)
                    }
                    if label != shown.last?.0 { Divider() }
                }
            }
        }
    }

    private func papers(_ papers: [HubPaper]) -> some View {
        Panel(title: "Research", systemImage: "text.book.closed") {
            ForEach(papers) { paper in
                VStack(alignment: .leading, spacing: 6) {
                    Link(paper.title ?? "arXiv \(paper.id)", destination: URL(string: "https://huggingface.co/papers/\(paper.id)")!)
                        .fontWeight(.medium)
                        .multilineTextAlignment(.leading)
                    HStack(spacing: 10) {
                        if !paper.authors.isEmpty {
                            Text(paper.authors.prefix(3).joined(separator: ", ") + (paper.authors.count > 3 ? " et al." : ""))
                                .lineLimit(1)
                        }
                        if let year = paper.publishedAt.flatMap(Naming.date) { Text(year.formatted(.dateTime.year())) }
                        if let upvotes = paper.upvotes, upvotes > 0 { Label("\(upvotes)", systemImage: "arrowtriangle.up") }
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    if let summary = paper.summary {
                        Text(summary).font(.caption).foregroundStyle(.secondary).lineLimit(4)
                    }
                    HStack(spacing: 14) {
                        Link("arXiv", destination: URL(string: "https://arxiv.org/abs/\(paper.id)")!)
                        Link("PDF", destination: URL(string: "https://arxiv.org/pdf/\(paper.id)")!)
                    }
                    .font(.caption)
                }
                if paper != papers.last { Divider() }
            }
        }
    }

    @ViewBuilder private func extras(_ card: HubModelCard) -> some View {
        let template = card.gguf?.chatTemplate
        if !card.datasets.isEmpty || !card.spaces.isEmpty || !card.tags.isEmpty || template != nil {
            Panel(title: "More", systemImage: "ellipsis.circle") {
                if !card.datasets.isEmpty { chips("Trained on", card.datasets) { Self.hub("datasets/\($0)") } }
                if !card.spaces.isEmpty { chips("Spaces using it", card.spaces) { Self.hub("spaces/\($0)") } }
                if !card.tags.isEmpty { chips("Tags", card.tags, link: nil) }
                if let template {
                    DisclosureGroup("Chat template") {
                        CodeSurface(text: template, language: "jinja", maxHeight: 260).padding(.top, 6)
                    }
                    .font(.callout)
                }
            }
        }
    }

    private func chips(_ label: String, _ items: [String], link: ((String) -> URL)?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            FlowLayout(spacing: 5) {
                ForEach(items, id: \.self) { item in
                    if let link {
                        Link(destination: link(item)) { Tag(item) }
                    } else {
                        Tag(item)
                    }
                }
            }
        }
    }

    // MARK: Actions

    private func load() async {
        do {
            let loaded = try await app.client.modelDetails(repo)
            card = loaded
            blocks = loaded.readme.map { ModelCardText.blocks($0, repo: loaded.repo) } ?? []
            failure = nil
        } catch {
            if card == nil { failure = error.localizedDescription }
        }
    }

    private func pull(_ file: HubFile) {
        guard let quant = file.quant else { return }
        pulling = true
        perform($pullFailure) {
            defer { pulling = false }
            try await app.client.pull("\(repo):\(quant)")
            app.invalidate(.models)
            downloads = (try? await app.client.downloads()) ?? downloads
        }
    }

    private static func hub(_ path: String) -> URL {
        URL(string: "https://huggingface.co/\(path)") ?? URL(string: "https://huggingface.co")!
    }

    /// `8190735360` → `8.2B`.
    private static func params(_ total: Int64) -> String {
        total >= 1_000_000_000 ? Naming.params(Double(total) / 1e9) : "\(total / 1_000_000)M"
    }
}

/// A titled surface on the detail screen.
private struct Panel<Content: View>: View {
    let title: String
    let systemImage: String
    var trailing: String?
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Label(title, systemImage: systemImage)
                    .font(.caption.weight(.semibold))
                    .textCase(.uppercase)
                    .foregroundStyle(.secondary)
                Spacer()
                if let trailing { Text(trailing).font(.caption).foregroundStyle(.tertiary) }
            }
            content
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.primary.opacity(0.08)))
    }
}

/// An image from a model card, at the size the card asked for or its own, never wider than the column.
private struct CardImageView: View {
    let image: CardImage
    /// Part of a row such as a badge strip, which keeps to text height.
    let inRow: Bool
    @State private var loaded: NSImage?

    var body: some View {
        Group {
            if let loaded {
                let drawn = Image(nsImage: loaded).resizable().interpolation(.high).scaledToFit()
                if inRow {
                    // Badges keep to one height and their own width, so a row packs like text.
                    drawn.frame(height: 24)
                } else {
                    drawn
                        .frame(maxWidth: image.width ?? loaded.size.width, maxHeight: 520)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
            } else {
                Color.clear.frame(width: inRow ? 60 : 1, height: inRow ? 20 : 1)
            }
        }
        .task(id: image.url) {
            guard let (data, _) = try? await URLSession.shared.data(from: image.url) else { return }
            loaded = NSImage(data: data)
        }
    }
}

/// A demo video from a model card. The player is kept across redraws so playback is not reset.
private struct CardVideo: View {
    let url: URL
    @State private var player: AVPlayer?

    var body: some View {
        VideoPlayer(player: player)
            .aspectRatio(16 / 9, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .onAppear { player = player ?? AVPlayer(url: url) }
    }
}
