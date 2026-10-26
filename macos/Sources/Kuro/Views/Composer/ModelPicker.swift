import SwiftUI
import KuroKit

/// The model chooser.
///
/// Local models come first, grouped by maker. Each provider is a closed
/// section, because one key can reach hundreds of models; searching opens
/// everything.
struct ModelPicker: View {
    @Environment(AppModel.self) private var app
    @Binding var selection: String?

    @State private var open = false
    @State private var query = ""
    @State private var expanded: Set<String> = []
    @State private var limits: [String: Int] = [:]

    /// Rows a provider shows before "Show more".
    private static let pageSize = 40

    private var active: String? { selection ?? app.models.fallbackId() }

    var body: some View {
        if app.models.readyLocal.isEmpty && app.models.remote.isEmpty {
            Button {
                app.go(.models)
            } label: {
                Label("No models yet", systemImage: "cube")
            }
            .buttonStyle(QuietButtonStyle())
        } else {
            Button {
                query = ""
                if let holder = app.models.remote.first(where: { $0.id == active }) {
                    expanded = ["remote:\(holder.connectorId)"]
                }
                open.toggle()
            } label: {
                HStack(spacing: 5) {
                    if let active, !Naming.isFree(active), Brands.model(Naming.friendly(active)) != nil {
                        ModelLogo(name: Naming.friendly(active), size: 14, bare: true)
                    } else {
                        Image(systemName: icon(for: active))
                    }
                    Text(active.map(Naming.friendly) ?? "Choose a model")
                        .lineLimit(1)
                        .truncationMode(.middle)
                    if let quant = active.flatMap(Naming.quant(of:)) {
                        Text(quant).font(.caption2.monospaced()).foregroundStyle(.tertiary)
                    }
                    Image(systemName: "chevron.up.chevron.down").font(.caption2)
                }
            }
            .buttonStyle(QuietButtonStyle())
            .help(active ?? "Choose a model")
            .popover(isPresented: $open, arrowEdge: .bottom) { menu }
        }
    }

    private var menu: some View {
        let groups = buildGroups()
        let searching = !query.trimmingCharacters(in: .whitespaces).isEmpty

        return VStack(spacing: 0) {
            TextField("Search every provider", text: $query)
                .textFieldStyle(.roundedBorder)
                .padding(10)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 1) {
                    if groups.isEmpty {
                        Text("Nothing matched.").foregroundStyle(.secondary).padding(12)
                    }
                    ForEach(Array(groups.enumerated()), id: \.element.key) { index, group in
                        if index == 0, !group.remote {
                            sectionHead("On This Mac")
                        } else if group.remote, index == 0 || !groups[index - 1].remote {
                            sectionHead("Providers")
                        }
                        groupView(group, searching: searching)
                    }
                }
                .padding(6)
            }
            Divider()
            Button("Manage Models…") {
                open = false
                app.go(.models)
            }
            .buttonStyle(.borderless)
            .padding(8)
        }
        .frame(width: 400)
        .frame(maxHeight: 480)
    }

    private func sectionHead(_ title: String) -> some View {
        Text(title)
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 8)
            .padding(.top, 8)
            .padding(.bottom, 2)
    }

    @ViewBuilder private func groupView(_ group: Group, searching: Bool) -> some View {
        let isOpen = !group.remote || searching || expanded.contains(group.key)
        let limit = searching ? Int.max : limits[group.key, default: Self.pageSize]
        let shown = group.options.prefix(limit)

        if group.remote {
            Button {
                withAnimation(.snappy) {
                    if expanded.contains(group.key) { expanded.remove(group.key) } else { expanded.insert(group.key) }
                }
            } label: {
                HStack(spacing: 6) {
                    if group.free {
                        Image(systemName: "gift").frame(width: 16)
                    } else {
                        ProviderLogo(slug: group.provider ?? "", label: group.label, size: 16, bare: true)
                    }
                    Text(group.label).fontWeight(.medium)
                    Text("\(group.options.count)").foregroundStyle(.tertiary).monospacedDigit()
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        } else {
            HStack(spacing: 6) {
                ModelLogo(name: group.label, owner: group.label, size: 16, bare: true)
                Text(group.label).fontWeight(.medium)
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
        }

        if isOpen {
            ForEach(shown) { option in row(option, in: group) }
            let hidden = group.options.count - shown.count
            if hidden > 0 {
                Button("Show \(min(hidden, Self.pageSize)) more · \(hidden) left") {
                    limits[group.key] = limit + Self.pageSize
                }
                .buttonStyle(.borderless)
                .font(.caption)
                .padding(.leading, 30)
                .padding(.vertical, 4)
            }
        }
    }

    private func row(_ option: Option, in group: Group) -> some View {
        HStack(spacing: 6) {
            Button {
                selection = option.id
                open = false
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "checkmark")
                        .font(.caption.weight(.bold))
                        .opacity(option.id == active ? 1 : 0)
                        .frame(width: 16)
                    if option.pooled {
                        Image(systemName: "gift").foregroundStyle(.secondary)
                    } else if !group.free {
                        optionLogo(option, in: group)
                    }
                    Text(option.name).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 8)
                    if let unavailable = option.unavailable {
                        Text(unavailable).font(.caption).foregroundStyle(.tertiary).lineLimit(1)
                    } else {
                        tags(option)
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
                .background(option.id == active ? Color.accentColor.opacity(0.12) : .clear, in: RoundedRectangle(cornerRadius: 7))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(option.unavailable != nil)
            .opacity(option.unavailable != nil ? 0.55 : 1)
            .help(option.unavailable.map { "\(option.name) — \($0)" } ?? option.id)

            if option.unavailable != nil, let fix = option.fixUrl.flatMap(URL.init(string:)) {
                Link(destination: fix) { Image(systemName: "arrow.up.right.square") }
                    .help("Open the page that enables this model")
            }
        }
    }

    @ViewBuilder private func tags(_ option: Option) -> some View {
        HStack(spacing: 4) {
            if let note = option.note { Text(note).font(.caption).foregroundStyle(.secondary) }
            // Cost first: it is the tag people scan for.
            if let free = option.free { Tag(free ? "free" : "paid", tone: free ? .live : .neutral) }
            ForEach(option.specialities, id: \.self) { Tag($0) }
            if let size = option.size { Text(size).font(.caption.monospaced()).foregroundStyle(.tertiary) }
            if let quant = option.quant { Text(quant).font(.caption2.monospaced()).foregroundStyle(.tertiary) }
            if option.loaded { Tag("loaded", tone: .live) }
        }
    }

    /// A provider's model whose name names no maker gets the provider's mark, so a remote row is never a bare letter.
    @ViewBuilder private func optionLogo(_ option: Option, in group: Group) -> some View {
        if group.remote && Brands.model(option.name) == nil {
            ProviderLogo(slug: group.provider ?? "", label: group.label, size: 14, bare: true)
        } else {
            ModelLogo(name: option.name, owner: group.remote ? nil : group.label, size: 14, bare: true)
        }
    }

    private func icon(for id: String?) -> String {
        guard let id else { return "cube" }
        if Naming.isFree(id) { return "gift" }
        return Naming.isRemote(id) ? "cloud" : "cube"
    }

    // MARK: Grouping

    private struct Option: Identifiable {
        let id: String
        let name: String
        var quant: String?
        var size: String?
        var loaded = false
        var pooled = false
        var note: String?
        /// `nil` for local models, which cost nothing either way.
        var free: Bool?
        var specialities: [String] = []
        var unavailable: String?
        var fixUrl: String?
    }

    private struct Group {
        let key: String
        let label: String
        let remote: Bool
        let free: Bool
        /// The provider's slug, for its logo. `nil` on a local group.
        var provider: String?
        let options: [Option]
    }

    /// Local models by maker, remote ones by provider; the free pool leads the providers.
    private func buildGroups() -> [Group] {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        let matches = { (text: String) in needle.isEmpty || text.lowercased().contains(needle) }

        var local: [String: [Option]] = [:]
        for entry in app.models.readyLocal where matches(entry.model.id) || matches(entry.model.displayName) {
            let brand = Naming.brand(of: entry.model.id, family: entry.model.family)
            let name = Naming.friendly(entry.model.id)
            local[brand, default: []].append(Option(
                id: entry.model.id,
                name: name.hasPrefix("\(brand)/") ? String(name.dropFirst(brand.count + 1)) : name,
                quant: Naming.quant(of: entry.model.id) ?? entry.model.quant?.uppercased(),
                size: entry.model.fileSizeBytes.map(Naming.bytes),
                loaded: entry.loaded,
                specialities: entry.kind != "chat" ? [entry.kind] : [],
                unavailable: entry.chat ? nil : "An \(entry.kind) model, not a chat model"
            ))
        }

        var remote: [(key: String, label: String, provider: String, options: [Option])] = []
        for model in app.models.remote where ([model.name, model.connectorLabel] + model.specialities).contains(where: matches) {
            let option = Option(
                id: model.id,
                name: model.name,
                size: model.paramsB.map(Naming.params),
                pooled: model.pooled,
                note: model.pooled ? "\(model.poolSize) free models" : nil,
                free: model.free,
                specialities: model.specialities.filter { $0 != "general" },
                unavailable: model.unavailable,
                fixUrl: model.fixUrl
            )
            if let index = remote.firstIndex(where: { $0.key == model.connectorId }) {
                remote[index].options.append(option)
            } else {
                remote.append((model.connectorId, model.connectorLabel, model.provider, [option]))
            }
        }

        let localGroups = local.keys.sorted().map { Group(key: "local:\($0)", label: $0, remote: false, free: false, options: local[$0]!) }
        let remoteGroups = remote
            .map { Group(key: "remote:\($0.key)", label: $0.label, remote: true, free: $0.key == "kuro-free",
                         provider: $0.provider, options: $0.options.sorted { $0.pooled && !$1.pooled }) }
            .sorted { $0.free && !$1.free }
        return localGroups + remoteGroups
    }
}

/// How hard to think, as one word and a chevron.
struct EffortPicker: View {
    @Binding var effort: Effort
    let coding: Bool
    let note: String
    var compact = false
    @State private var open = false

    var body: some View {
        // `ultra` carried into a chat falls back to Balanced.
        let current = Effort.options(coding: coding).contains(effort) ? effort : .balanced
        Button {
            open.toggle()
        } label: {
            HStack(spacing: 4) {
                Image(systemName: current.thinks ? "brain" : "bolt")
                if !compact { Text(current.label) }
                Image(systemName: "chevron.down").font(.caption2)
            }
            .foregroundStyle(current.thinks ? Color.accentColor : .secondary)
        }
        .buttonStyle(QuietButtonStyle())
        .help(current.detail)
        .popover(isPresented: $open, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 4) {
                Text("How hard to think")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                ForEach(Effort.options(coding: coding), id: \.self) { option in
                    Button {
                        effort = option
                        open = false
                    } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 8) {
                            Image(systemName: "checkmark")
                                .font(.caption.weight(.bold))
                                .opacity(option == current ? 1 : 0)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(option.label).fontWeight(.medium)
                                Text(option.detail)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                        .padding(8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(option == current ? Color.accentColor.opacity(0.1) : .clear, in: RoundedRectangle(cornerRadius: 8))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                Divider().padding(.vertical, 4)
                Text(note)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 8)
            }
            .padding(10)
            .frame(width: 320)
        }
    }
}
