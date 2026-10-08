import SwiftUI
import RodaCore

/// Start a chat with someone by their @, make a group, or join one with an invite code.
/// People search and invites go through the relay; the chat itself is created on this
/// device first (signed, queued) and works offline.
struct NewChatSheet: View {
    enum Mode: String, CaseIterable, Identifiable {
        case chat, group, join
        var id: String { rawValue }
        var title: String {
            switch self {
            case .chat: String(localized: "Chat")
            case .group: String(localized: "Group")
            case .join: String(localized: "Join")
            }
        }
    }

    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var mode: Mode = .chat
    @State private var query = ""
    @State private var results: [Persona] = []
    @State private var searching = false
    @State private var searchError: String?
    @State private var picked: [Persona] = []
    @State private var groupTitle = ""
    @State private var code = ""
    @State private var preview: InvitePreviewDto?
    @State private var working = false
    @State private var failure: String?

    init(mode: Mode = .chat) {
        _mode = State(initialValue: mode)
    }

    var body: some View {
        NavigationStack {
            Form {
                Picker("", selection: $mode) {
                    ForEach(Mode.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .listRowBackground(Color.clear)

                if !model.sync.isOnline {
                    Label { Text("You’re offline. Finding people and invites need a connection; chats you start still go out when you’re back.") } icon: { Image(systemName: "wifi.slash") }
                        .font(.footnote)
                        .foregroundStyle(Palette.textSecondary)
                }

                switch mode {
                case .chat, .group: peopleSection
                case .join: joinSection
                }

                if let failure {
                    Text(failure).font(.footnote).foregroundStyle(Palette.danger)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(mode == .join ? String(localized: "Join a group") : String(localized: "New chat"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                if mode == .group {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Create") { createGroup() }
                            .disabled(working || picked.isEmpty || groupTitle.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
            .task(id: query) { await search() }
            .task(id: code) { await loadPreview() }
        }
        #if os(macOS)
        .frame(minWidth: 420, minHeight: 480)
        #endif
    }

    // MARK: people

    @ViewBuilder
    private var peopleSection: some View {
        if mode == .group {
            Section {
                TextField(String(localized: "Group name"), text: $groupTitle)
                if !picked.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(picked) { p in
                                Button { picked.removeAll { $0.id == p.id } } label: {
                                    HStack(spacing: 6) {
                                        Avatar(persona: p, size: 22)
                                        Text(p.name).font(.subheadline.weight(.medium))
                                        Image(systemName: "xmark").font(.caption2.weight(.bold))
                                    }
                                    .padding(.horizontal, 10).padding(.vertical, 6)
                                    .background(Palette.action.opacity(0.12), in: .capsule)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
            }
        }
        Section {
            HStack {
                Text(verbatim: "@").foregroundStyle(Palette.textTertiary)
                TextField(String(localized: "Search by @handle"), text: $query)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.never)
                    #endif
                if searching { ProgressView().controlSize(.small) }
            }
            ForEach(results) { p in
                Button { tapped(p) } label: {
                    HStack(spacing: 12) {
                        Avatar(persona: p, size: 36)
                        VStack(alignment: .leading, spacing: 1) {
                            Text(p.name).font(.body.weight(.medium)).foregroundStyle(Palette.textPrimary)
                            Text("@\(p.handle)").font(.caption).foregroundStyle(Palette.textSecondary)
                        }
                        Spacer()
                        if mode == .group, picked.contains(where: { $0.id == p.id }) {
                            Image(systemName: "checkmark.circle.fill").foregroundStyle(Palette.action)
                        }
                    }
                    .contentShape(.rect)   // the whole row, not just the name, is the button
                }
                .buttonStyle(.plain)
                .disabled(working)
            }
            if let searchError {
                Text(searchError).font(.footnote).foregroundStyle(Palette.textSecondary)
            } else if !query.isEmpty && !searching && results.isEmpty {
                Text("Nobody with that @ yet.").font(.footnote).foregroundStyle(Palette.textSecondary)
            }
        } footer: {
            if let me = model.sync.account {
                Text("People find you as @\(me.handle).")
            }
        }
    }

    private func search() async {
        let q = query.trimmingCharacters(in: .whitespaces).trimmingCharacters(in: CharacterSet(charactersIn: "@"))
        guard q.count >= 2 else { results = []; searchError = nil; return }
        try? await Task.sleep(for: .milliseconds(250))
        guard !Task.isCancelled else { return }
        searching = true
        defer { searching = false }
        do {
            let found = try await model.core.findPeople(query: q)
            guard !Task.isCancelled else { return }
            results = found.filter { !$0.isMe }
            searchError = nil
        } catch {
            results = []
            searchError = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    private func tapped(_ p: Persona) {
        failure = nil
        if mode == .group {
            if let i = picked.firstIndex(where: { $0.id == p.id }) { picked.remove(at: i) } else { picked.append(p) }
            return
        }
        working = true
        defer { working = false }
        do {
            let id = try model.core.startDirect(identityId: p.id)
            model.refresh()
            open(id)
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    private func createGroup() {
        working = true
        defer { working = false }
        let title = groupTitle.trimmingCharacters(in: .whitespaces)
        do {
            let id = try model.core.createGroup(title: title, memberIds: picked.map(\.id))
            model.refresh()
            open(id)
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    // MARK: join

    @ViewBuilder
    private var joinSection: some View {
        Section {
            TextField(String(localized: "Invite code"), text: $code)
                .font(.body.monospaced())
                .autocorrectionDisabled()
                #if os(iOS)
                .textInputAutocapitalization(.characters)
                #endif
            if let preview {
                HStack(spacing: 12) {
                    GlossySphere(seed: preview.title, size: 40)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(preview.title).font(.body.weight(.semibold))
                        Text(preview.inviter.map { String(localized: "\($0.name) invited you · \(preview.members) members") } ?? String(localized: "\(preview.members) members"))
                            .font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                }
                Button { Task { await join() } } label: {
                    Text("Join \(preview.title)").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(working)
            }
        } footer: {
            Text("Paste the code (or the zoen://join link) someone shared with you.")
        }
    }

    private func loadPreview() async {
        preview = nil
        failure = nil
        let c = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard c.count >= 6 else { return }
        try? await Task.sleep(for: .milliseconds(300))
        guard !Task.isCancelled else { return }
        do {
            preview = try await model.core.previewInvite(code: c)
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    private func join() async {
        working = true
        defer { working = false }
        do {
            let id = try await model.core.joinInvite(code: code)
            model.refresh()
            open(id)
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }

    private func open(_ spaceId: String) {
        dismiss()
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(350))
            model.go(.space(spaceId))
        }
    }
}
