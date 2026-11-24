import SwiftUI
import KuroKit

/// Tokens spent per model, local and remote, over a chosen window.
struct UsageSettings: View {
    @Environment(AppModel.self) private var app
    @State private var days = 30
    @State private var report: UsageReport?
    @State private var failure: String?

    var body: some View {
        Form {
            Section {
                Picker("Window", selection: $days) {
                    Text("Today").tag(1)
                    Text("7 days").tag(7)
                    Text("30 days").tag(30)
                    Text("Year").tag(365)
                }
                .pickerStyle(.segmented)
                .labelsHidden()

                if let report {
                    HStack(spacing: 0) {
                        figure("Tokens", report.total.totalTokens.formatted())
                        figure("Replies", report.total.turns.formatted())
                        figure("Per reply", report.total.turns > report.total.unreportedTurns
                               ? (report.total.totalTokens / (report.total.turns - report.total.unreportedTurns)).formatted()
                               : "—")
                    }
                }
                if let failure { ErrorText(failure) }
            }

            if let report {
                Section {
                    if report.models.isEmpty {
                        Text("Nothing in this window.").foregroundStyle(.secondary)
                    }
                    let largest = max(report.models.map(\.totalTokens).max() ?? 1, 1)
                    ForEach(report.models) { row in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack(spacing: 8) {
                                ModelLogo(name: row.modelId, size: 20, bare: true)
                                Text(Naming.friendly(row.modelId)).lineLimit(1)
                                Tag(Naming.isRemote(row.modelId) ? "remote" : "this Mac", tone: Naming.isRemote(row.modelId) ? .neutral : .live)
                                Spacer()
                                Text(row.totalTokens.formatted()).font(.callout.monospacedDigit()).foregroundStyle(.secondary)
                            }
                            ProgressView(value: Double(row.totalTokens), total: Double(largest))
                                .tint(.primary.opacity(0.6))
                            Text("\(row.turns) \(row.turns == 1 ? "reply" : "replies") · \(row.promptTokens.formatted()) in · \(row.completionTokens.formatted()) out")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .help(row.unreportedTurns > 0 ? "\(row.unreportedTurns) replies reported no token counts" : row.modelId)
                    }
                } header: {
                    Text("By Model")
                }
            }
        }
        .settingsForm()
        .task(id: days) {
            do {
                report = try await app.client.usage(days: days)
                failure = nil
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    private func figure(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(.secondary)
            Text(value).font(.title3.weight(.semibold)).monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
