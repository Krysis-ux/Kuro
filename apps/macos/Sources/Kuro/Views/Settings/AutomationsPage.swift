import SwiftUI
import KuroKit

/// Saved prompts that run on a schedule. Each run's reply lands in the automation's own chat.
struct AutomationsPage: View {
    @Environment(AppModel.self) private var app
    @State private var automations: [Automation] = []
    @State private var loaded = false
    @State private var editing: EditRequest?
    @State private var failure: String?

    struct EditRequest: Identifiable {
        let id = UUID()
        let automation: Automation?
    }

    private var anyRunning: Bool { automations.contains(where: \.running) }

    var body: some View {
        SettingsPage(pane: .automations) {
            Button { editing = EditRequest(automation: nil) } label: { Label("New", systemImage: "plus") }
                .buttonStyle(PrimaryButtonStyle())
        } content: {
            Form {
                Section {
                    if let failure { ErrorText(failure) }
                    if loaded && automations.isEmpty {
                        Text("None yet. Save a prompt and when it should run.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(automations) { row($0) }
                }
            }
            .settingsForm()
        }
        .task(id: app.server.phase) { await load() }
        // While a run is under way, check back so its row and its chat catch up when it ends.
        .task(id: anyRunning) {
            while anyRunning, !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                await load()
            }
        }
        .sheet(item: $editing) { request in
            AutomationEditor(original: request.automation) {
                editing = nil
                Task { await load() }
            }
        }
    }

    private func row(_ automation: Automation) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(automation.name)
                Text(detail(automation))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if let error = automation.lastError { ErrorText(error).lineLimit(2) }
            }
            Spacer()
            if let chat = automation.conversationId {
                Button { app.go(.chat(chat)) } label: { Image(systemName: "bubble.left") }
                    .buttonStyle(.borderless)
                    .help("Open its chat")
            }
            Button(automation.running ? "Running…" : "Run Now") { run(automation) }
                .disabled(automation.running)
            Button("Edit") { editing = EditRequest(automation: automation) }
            Button { remove(automation) } label: { Image(systemName: "trash") }
                .buttonStyle(.borderless)
                .help("Delete. Its chat stays.")
            Toggle("On", isOn: Binding(get: { automation.enabled }, set: { setEnabled(automation, $0) }))
                .toggleStyle(.switch)
                .controlSize(.small)
                .labelsHidden()
        }
    }

    /// `Daily at 09:00 · Last run 2h ago`.
    private func detail(_ automation: Automation) -> String {
        let schedule = AutomationSchedule(automation.schedule)?.label ?? automation.schedule
        let last = automation.lastRunAt.map { "Last run " + Naming.relative($0) } ?? "Not run yet"
        return "\(schedule) · \(last)"
    }

    private func load() async {
        guard app.server.phase == .ready else { return }
        do {
            let list = try await app.client.automations()
            // Runs that ended since the last look: their chats have a new reply.
            let ended = automations.filter { old in old.running && !list.contains { $0.id == old.id && $0.running } }
            automations = list
            loaded = true
            if !ended.isEmpty {
                // Unstructured, so a page task cancelled as `anyRunning` flips does not cancel the reload.
                for chat in list.filter({ run in ended.contains { $0.id == run.id } }).compactMap(\.conversationId) {
                    Task { await app.loadMessages(chat) }
                }
                app.invalidate(.conversations)
            }
        } catch {
            failure = error.localizedDescription
        }
    }

    private func run(_ automation: Automation) {
        perform($failure) {
            _ = try await app.client.runAutomation(automation.id)
            app.invalidate(.conversations)
            await load()
        }
    }

    private func setEnabled(_ automation: Automation, _ enabled: Bool) {
        perform($failure) {
            try await app.client.updateAutomation(automation.id, [
                "name": automation.name,
                "prompt": automation.prompt,
                "model": automation.model,
                "schedule": automation.schedule,
                "enabled": enabled,
            ])
            await load()
        }
    }

    private func remove(_ automation: Automation) {
        perform($failure) {
            try await app.client.deleteAutomation(automation.id)
            await load()
        }
    }
}

private struct AutomationEditor: View {
    @Environment(AppModel.self) private var app
    let original: Automation?
    let onClose: () -> Void

    enum Repeat: Hashable { case every, daily, weekdays }

    @State private var name = ""
    @State private var prompt = ""
    @State private var model: String?
    @State private var repeats = Repeat.daily
    @State private var minutes = 60
    @State private var time = AutomationEditor.clock(9, 0)
    @State private var failure: String?
    @State private var saving = false

    private static let intervals = [5, 10, 15, 30, 60, 120, 180, 360, 720]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(original == nil ? "New Automation" : "Edit Automation")
                .font(.system(size: 16, weight: .semibold))
                .padding([.horizontal, .top], 20)
                .padding(.bottom, 12)

            Form {
                Section {
                    TextField("Name", text: $name, prompt: Text("Morning briefing"))
                    LabeledContent {
                        // Spelled out, since the picker on its own would show its fallback, not the server's default.
                        HStack {
                            if model == nil {
                                Text("Default").foregroundStyle(.secondary)
                                Button("Choose…") { model = app.models.fallbackId() }.buttonStyle(.borderless)
                            } else {
                                ModelPicker(selection: $model)
                                Button("Use Default") { model = nil }.buttonStyle(.borderless)
                            }
                        }
                    } label: {
                        TitleWithHelp("Model", help: "Left alone, each run uses whatever Kuro's default model is at the time.")
                    }
                }
                Section {
                    Picker("Repeat", selection: $repeats) {
                        Text("Every…").tag(Repeat.every)
                        Text("Daily").tag(Repeat.daily)
                        Text("Weekdays").tag(Repeat.weekdays)
                    }
                    if repeats == .every {
                        Picker("Every", selection: $minutes) {
                            ForEach(Array(Set(Self.intervals + [minutes])).sorted(), id: \.self) { value in
                                Text(Self.interval(value)).tag(value)
                            }
                        }
                    } else {
                        DatePicker("At", selection: $time, displayedComponents: .hourAndMinute)
                    }
                } header: {
                    TitleWithHelp("Schedule", help: "This Mac's time. A run missed while it slept or Kuro was closed happens once when it is back.")
                }
                Section {
                    TextEditor(text: $prompt)
                        .font(.body)
                        .frame(minHeight: 120)
                        .scrollContentBackground(.hidden)
                } header: {
                    TitleWithHelp("Prompt", help: "Sent as a new message each run. The replies collect in one chat named after the automation.")
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)

            HStack {
                if let failure { ErrorText(failure).lineLimit(2) }
                Spacer()
                Button("Cancel", action: onClose)
                    .buttonStyle(SubtleButtonStyle())
                    .keyboardShortcut(.cancelAction)
                Button(saving ? "Saving…" : "Save", action: save)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || saving)
            }
            .padding(20)
        }
        .frame(width: 520, height: 600)
        .background(Palette.canvas)
        .onAppear(perform: fill)
    }

    private var schedule: AutomationSchedule {
        let parts = Calendar.current.dateComponents([.hour, .minute], from: time)
        let (hour, minute) = (parts.hour ?? 9, parts.minute ?? 0)
        switch repeats {
        case .every: return .every(minutes: minutes)
        case .daily: return .daily(hour: hour, minute: minute)
        case .weekdays: return .weekdays(hour: hour, minute: minute)
        }
    }

    private func fill() {
        guard let original else { return }
        name = original.name
        prompt = original.prompt
        model = original.model
        switch AutomationSchedule(original.schedule) {
        case .every(let value): repeats = .every; minutes = value
        case .daily(let hour, let minute): repeats = .daily; time = Self.clock(hour, minute)
        case .weekdays(let hour, let minute): repeats = .weekdays; time = Self.clock(hour, minute)
        case nil: break
        }
    }

    private func save() {
        let body: KuroClient.Body = [
            "name": name.trimmingCharacters(in: .whitespaces),
            "prompt": prompt,
            "model": model,
            "schedule": schedule.text,
            "enabled": original?.enabled ?? true,
        ]
        saving = true
        perform($failure) {
            defer { saving = false }
            if let original {
                try await app.client.updateAutomation(original.id, body)
            } else {
                try await app.client.createAutomation(body)
            }
            onClose()
        }
    }

    private static func clock(_ hour: Int, _ minute: Int) -> Date {
        Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: Date()) ?? Date()
    }

    /// `30 minutes`, `1 hour`, `6 hours`.
    private static func interval(_ minutes: Int) -> String {
        if minutes % 60 != 0 { return "\(minutes) minutes" }
        return minutes == 60 ? "1 hour" : "\(minutes / 60) hours"
    }
}
