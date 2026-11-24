import SwiftUI
import KuroKit

/// Kuro Free: your own free-tier keys pooled into one model, which sends each
/// request to whichever provider still has allowance. Shown inside Providers.
struct FreeDetail: View {
    @Environment(AppModel.self) private var app
    @State private var overview: FreeOverview?
    @State private var usage: FreeUsage?
    @State private var failure: String?

    var body: some View {
        Form {
            Section {
                HStack(spacing: 12) {
                    Image(systemName: "gift")
                        .font(.system(size: 16))
                        .frame(width: 36, height: 36)
                        .background(Palette.sunken, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 6) {
                            Text("Kuro Free").font(.system(size: 16, weight: .semibold))
                            HelpTip("Kuro supplies no keys. Each one is yours, on a free tier, in your own name. Requests leave this Mac, and free tiers are the likeliest to train on them — keep private work local.")
                        }
                        Text(statusLine).font(.callout).foregroundStyle(.secondary)
                    }
                }
                if let overview, overview.keyCount > 0 {
                    ForEach(overview.flavours) { flavour in
                        LabeledContent {
                            Tag(flavour.available ? "ready" : "no provider", tone: flavour.available ? .live : .neutral)
                        } label: {
                            Text(flavour.id).font(.body.monospaced())
                        }
                        .help(flavour.blurb)
                    }
                }
            }

            if let failure { Section { ErrorText(failure) } }

            if let overview {
                let owned = overview.providers.filter { !$0.keyless }
                let withKeys = owned.filter(\.hasKey)
                let without = owned.filter { !$0.hasKey }
                let shared = overview.providers.filter(\.keyless)

                if !withKeys.isEmpty {
                    Section("Your Keys") {
                        ForEach(withKeys) { FreeProviderRow(provider: $0) }
                    }
                }

                Section {
                    ForEach(without) { FreeProviderRow(provider: $0) }
                } header: {
                    TitleWithHelp(withKeys.isEmpty ? "Add a Free Key" : "Add More", help: "Each is free to sign up for and needs no card. More keys make the pool sturdier: when one runs out, the next answers.")
                }

                // Listed last because they are tried last.
                if !shared.isEmpty {
                    Section {
                        Toggle(isOn: Binding(
                            get: { overview.allowKeyless },
                            set: { on in
                                perform($failure) {
                                    try await app.client.setKeyless(on)
                                    app.invalidate(.free, .models)
                                }
                            }
                        )) {
                            TitleWithHelp("Use shared endpoints as a last resort", help: "These answer with no account, so Kuro Free works before you sign up for anything. They are rate limited, often busy, and several keep or train on what they are sent.")
                        }
                        ForEach(shared) { FreeProviderRow(provider: $0) }
                    } header: {
                        Text("Shared Endpoints")
                    }
                }
            }

            if let usage { UsageSection(usage: usage) }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .task(id: app.stamp(.free)) {
            do {
                overview = try await app.client.free()
                failure = nil
            } catch {
                failure = error.localizedDescription
            }
            usage = try? await app.client.freeUsage()
        }
    }

    private var statusLine: String {
        guard let overview else { return "Loading…" }
        if overview.keyCount == 0 { return "Add a free key to turn it on." }
        return "\(overview.availableCount) of \(overview.keyCount) \(overview.keyCount == 1 ? "key" : "keys") working"
    }
}

private struct FreeProviderRow: View {
    @Environment(AppModel.self) private var app
    let provider: FreeProvider

    @State private var key = ""
    @State private var editing = false
    @State private var testing = false
    @State private var result: KeyTest?
    @State private var limit = ""
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                ProviderLogo(slug: provider.slug, label: provider.name, size: 24)
                StatusDot(color: provider.trouble != nil ? .red : provider.hasKey ? .green : .secondary.opacity(0.4))
                Text(provider.name).fontWeight(.medium)
                switch provider.trouble {
                case "rate_limited": Tag("used up", tone: .warning).help("Out of allowance for now")
                case "rejected": Tag("key refused", tone: .warning)
                case "model_gone": Tag("rechecking", tone: .warning).help("Its models moved; Kuro is rechecking")
                default: EmptyView()
                }
                if provider.expired { Tag("trial ended", tone: .warning) }
                if provider.privacy.trains {
                    Tag("may train", tone: .warning).help("Prompts sent here may be kept and used to train models")
                } else if provider.privacy.logged {
                    Tag("logged").help("Prompts sent here may be logged")
                }
                Spacer()
                if provider.hasKey { actions }
            }

            Text(provider.allowance).font(.callout).foregroundStyle(.secondary).lineLimit(1).help(provider.allowance)

            if provider.baseUrl.contains("ACCOUNT_ID"), provider.hasKey {
                ErrorText("Needs your account id. Add it as a custom provider instead.")
            }

            // A shared endpoint takes no key, and the server refuses one.
            if !provider.keyless, editing || !provider.hasKey {
                HStack {
                    SecureField("Key", text: $key, prompt: Text(provider.keyHint ?? "Paste the key"))
                        .labelsHidden()
                        .onSubmit(save)
                    Button("Save", action: save).disabled(key.trimmingCharacters(in: .whitespaces).isEmpty)
                    if editing { Button("Cancel") { editing = false } }
                }
            }

            if provider.hasKey {
                HStack {
                    Text("Monthly limit").font(.caption).foregroundStyle(.secondary)
                    TextField("Tokens", text: $limit, prompt: Text("e.g. 1000000"))
                        .labelsHidden()
                        .frame(maxWidth: 160)
                        .onSubmit(saveLimit)
                    Text("tokens").font(.caption).foregroundStyle(.secondary)
                }
            }

            if let result {
                if result.ok {
                    Label("Working.", systemImage: "checkmark.circle.fill").foregroundStyle(.green).font(.callout)
                } else {
                    ErrorText(result.error ?? "The key did not work.")
                }
            }
            if let failure { ErrorText(failure) }

            if !provider.hasKey, let url = URL(string: provider.credentialsUrl) {
                Link(provider.keyless ? "How this endpoint works" : "Get a free key", destination: url).font(.caption)
            }
        }
        .padding(.vertical, 4)
        .opacity(provider.hasKey || provider.keyless ? 1 : 0.85)
        .onAppear { limit = provider.limit?.tokensPerMonth.map(String.init) ?? "" }
    }

    private var actions: some View {
        HStack(spacing: 4) {
            Button(testing ? "Testing…" : "Test", action: test).disabled(testing)
                .help("Ask this provider whether the key works")
            Button { editing.toggle() } label: { Image(systemName: "arrow.triangle.2.circlepath") }
                .help("Replace the key")
            Button(role: .destructive) {
                perform($failure) {
                    try await app.client.removeFreeKey(provider.slug)
                    result = nil
                    app.invalidate(.free, .models)
                }
            } label: { Image(systemName: "trash") }
                .help("Remove the key")
        }
        .buttonStyle(.borderless)
        .controlSize(.small)
    }

    private func save() {
        let trimmed = key.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return }
        perform($failure) {
            try await app.client.setFreeKey(provider.slug, key: trimmed)
            key = ""
            editing = false
            result = nil
            app.invalidate(.free, .models)
        }
    }

    private func test() {
        testing = true
        perform($failure) {
            defer { testing = false }
            result = try await app.client.testFreeKey(provider.slug)
            app.invalidate(.free, .models)
        }
    }

    /// An empty or non-numeric value clears the limit.
    private func saveLimit() {
        let digits = limit.filter(\.isNumber)
        perform($failure) {
            try await app.client.setFreeLimit(provider.slug, tokensPerMonth: Int(digits).flatMap { $0 > 0 ? $0 : nil })
            app.invalidate(.free)
        }
    }
}

/// What the keys have been spent on, measured from the counts providers return.
private struct UsageSection: View {
    let usage: FreeUsage

    var body: some View {
        Section {
            if usage.month.turns == 0 {
                Text("Nothing used this month.")
                    .foregroundStyle(.secondary)
            } else {
                HStack(spacing: 0) {
                    figure("Today", usage.day.totalTokens.formatted(), "tokens")
                    figure("This month", usage.month.totalTokens.formatted(), "tokens")
                    figure("Per message", usage.averages.tokensPerTurn.map { Int($0).formatted() } ?? "—", "average")
                    figure("Per day", usage.averages.tokensPerDayThisMonth.map { Int($0).formatted() } ?? "—", "this month")
                }
                ForEach(usage.month.providers) { row in
                    let limit = row.limit?.tokensPerMonth
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(row.name)
                            Spacer()
                            Text(limit.map { "\(row.totalTokens.formatted()) / \($0.formatted())" } ?? row.totalTokens.formatted())
                                .font(.callout.monospaced())
                                .foregroundStyle(.secondary)
                        }
                        // No limit entered means no bar: Kuro cannot discover these allowances.
                        if let limit, limit > 0 {
                            ProgressView(value: min(1, Double(row.totalTokens) / Double(limit)))
                        }
                        Text("\(row.turns) \(row.turns == 1 ? "message" : "messages")")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        } header: {
            if usage.month.unreportedTurns > 0 {
                TitleWithHelp("Usage", help: "\(usage.month.unreportedTurns) \(usage.month.unreportedTurns == 1 ? "turn" : "turns") this month returned no token counts, so the totals are a floor.")
            } else {
                Text("Usage")
            }
        }
    }

    private func figure(_ label: String, _ value: String, _ unit: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(.secondary)
            Text(value).font(.title3.weight(.semibold)).monospacedDigit()
            Text(unit).font(.caption2).foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
