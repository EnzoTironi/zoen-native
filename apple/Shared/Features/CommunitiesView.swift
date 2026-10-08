import SwiftUI
import RodaCore

/// Spaces: your communities, same family as Chats and Store. Header matches Store;
/// rows match the chat list; create/join are real (core), never stubs.
struct CommunitiesScreen: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @State private var creating = false
    @State private var joining = false

    private var spaces: [SpaceSummary] {
        let q = query.trimmingCharacters(in: .whitespaces)
            .folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
        let all = model.spaces.filter { $0.kind == .community }
        guard !q.isEmpty else { return all.sorted { $0.lastAtMs > $1.lastAtMs } }
        return all.filter {
            ([ $0.title, $0.lastPreview ] + $0.members.map(\.name))
                .joined(separator: " ")
                .folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
                .contains(q)
        }
    }

    var body: some View {
        let searching = !query.isEmpty
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if !searching {
                    TabHeader(title: AppTab.communities.title) {
                        Button {
                            Haptics.tap()
                            creating = true
                        } label: {
                            ZoenIcon(.plus, size: 18)
                                .foregroundStyle(Palette.textPrimary)
                                .frame(width: 40, height: 40)
                                .glassEffect(.regular.interactive(), in: .circle)
                        }
                        .buttonStyle(IconPressStyle())
                        .accessibilityLabel(String(localized: "Create a Space"))
                        .accessibilityIdentifier("createSpace")
                    }
                    .padding(.horizontal, 20)
                    .padding(.top, 4)
                    .transition(.opacity.combined(with: .move(edge: .top)))
                }

                if spaces.isEmpty {
                    if searching {
                        InkEmptyState(pose: .map, title: String(localized: "Nothing for “\(query)”"))
                    } else {
                        InkEmptyState(
                            pose: .cheer,
                            title: String(localized: "No Spaces yet"),
                            message: String(localized: "Create one for a club, a trip, or a neighbourhood — or join with an invite.")
                        )
                        .padding(.top, 24)
                        HStack(spacing: 12) {
                            Button {
                                Haptics.tap()
                                creating = true
                            } label: {
                                Label(String(localized: "Create"), systemImage: "plus")
                                    .font(.subheadline.weight(.semibold))
                                    .frame(maxWidth: .infinity)
                                    .frame(height: 44)
                            }
                            .buttonStyle(.glassProminent)
                            .tint(Palette.action)
                            .accessibilityIdentifier("createSpaceEmpty")

                            Button {
                                Haptics.tap()
                                joining = true
                            } label: {
                                Label(String(localized: "Join"), systemImage: "person.badge.plus")
                                    .font(.subheadline.weight(.semibold))
                                    .frame(maxWidth: .infinity)
                                    .frame(height: 44)
                            }
                            .buttonStyle(.glass)
                            .accessibilityIdentifier("joinSpaceEmpty")
                        }
                        .padding(.horizontal, 20)
                    }
                } else {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(spaces) { space in
                            Button { model.push(.space(space.id)) } label: {
                                SpaceRow(space: space, pinned: model.isPinned(space))
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                Button {
                                    withAnimation(.spring(duration: 0.4)) { model.togglePin(space) }
                                } label: {
                                    Label {
                                        Text(model.isPinned(space) ? "Unpin" : "Pin")
                                    } icon: { ZoenGlyph.pin.menuImage }
                                }
                                Button {
                                    model.perform { try model.core.markRead(spaceId: space.id) }
                                } label: {
                                    Label { Text("Mark as read") } icon: { ZoenGlyph.check.menuImage }
                                }
                            }
                        }
                    }
                }
            }
            .padding(.top, searching ? 12 : 0)
            .padding(.bottom, 110)
            .animation(.spring(duration: 0.4, bounce: 0.15), value: searching)
        }
        .scrollDismissesKeyboard(.interactively)
        .scrollEdgeEffectStyle(.soft, for: .top)
        .background(NightBackdrop())
        .navigationTitle(AppTab.communities.title)
        #if os(iOS)
        .toolbar(.hidden, for: .navigationBar)
        #endif
        .sheet(isPresented: $creating) {
            CreateSpaceSheet()
                .presentationDetents([.height(280)])
        }
        .sheet(isPresented: $joining) {
            NewChatSheet(mode: .join)
        }
    }
}

/// One Space in the list: same anatomy as a chat row (avatar, title, preview, time, unread).
struct SpaceRow: View {
    let space: SpaceSummary
    var pinned = false

    var body: some View {
        let unread = space.unread > 0
        let people = space.members.filter { $0.kind != .agent }
        HStack(alignment: .center, spacing: 14) {
            ChatAvatar(space: space, size: 56)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(space.title)
                        .font(.body.weight(unread ? .bold : .medium))
                        .foregroundStyle(Palette.textPrimary)
                        .lineLimit(1)
                    Spacer(minLength: 6)
                    Text(RodaTime.short(space.lastAtMs))
                        .font(.footnote)
                        .foregroundStyle(Palette.textTertiary)
                }
                HStack(alignment: .center, spacing: 6) {
                    Text(preview)
                        .font(.subheadline)
                        .foregroundStyle(unread ? Palette.textPrimary.opacity(0.8) : Palette.textSecondary)
                        .lineLimit(1)
                    Spacer(minLength: 6)
                    if pinned {
                        ZoenIcon(.pin, size: 13).foregroundStyle(Palette.textTertiary)
                    }
                    if unread {
                        Circle().fill(Palette.action).frame(width: 10, height: 10)
                            .transition(.scale.combined(with: .opacity))
                    }
                }
                if people.count > 1 {
                    FacePile(members: people, size: 18)
                        .padding(.top, 2)
                        .accessibilityLabel(String(localized: "\(people.count) people"))
                }
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 10)
        .contentShape(.rect)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(a11y)
        .accessibilityIdentifier("spaceRow-\(space.title)")
    }

    private var preview: String {
        if let a = space.lastAuthor {
            if a.isMe { return String(localized: "You: ") + (VoiceNoteRef.preview(space.lastPreview) ?? space.lastPreview) }
            return "\(a.name): \(VoiceNoteRef.preview(space.lastPreview) ?? space.lastPreview)"
        }
        return space.lastPreview.isEmpty
            ? String(localized: "No messages yet")
            : (VoiceNoteRef.preview(space.lastPreview) ?? space.lastPreview)
    }

    private var a11y: String {
        var parts = [space.title, String(localized: "Space")]
        if pinned { parts.append(String(localized: "Pinned")) }
        if space.unread > 0 { parts.append(String(localized: "\(space.unread) unread")) }
        parts.append(preview)
        parts.append(RodaTime.short(space.lastAtMs))
        return parts.joined(separator: ", ")
    }
}

/// Name a Space and create it through the core.
struct CreateSpaceSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var working = false
    @State private var failure: String?
    @State private var revealing: (id: String, title: String)?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(String(localized: "Space name"), text: $title)
                        .accessibilityIdentifier("spaceNameField")
                } footer: {
                    Text(String(localized: "A place for a club, a trip or a neighbourhood. You can invite people after."))
                }
                if let failure {
                    Text(failure).font(.footnote).foregroundStyle(Palette.danger)
                }
            }
            .navigationTitle(String(localized: "New Space"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") { create() }
                        .disabled(working || title.trimmingCharacters(in: .whitespaces).isEmpty)
                        .accessibilityIdentifier("confirmCreateSpace")
                }
            }
            .overlay {
                if let revealing {
                    ZStack {
                        InkPalette.paper.opacity(0.92).ignoresSafeArea()
                        VStack(spacing: 16) {
                            SpaceArtReveal(doodle: .trip, accent: InkPalette.sky, size: 120,
                                           art: HandDrawnAvatarAsset.pickGroup(spaceId: revealing.id, title: revealing.title)) {
                                let id = revealing.id
                                dismiss()
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                                    model.push(.space(id))
                                }
                            }
                            Text(revealing.title)
                                .font(.title3.weight(.bold))
                                .foregroundStyle(InkPalette.ink)
                        }
                    }
                    .transition(.opacity)
                }
            }
            .animation(.easeOut(duration: 0.25), value: revealing?.id)
        }
    }

    private func create() {
        working = true
        defer { working = false }
        let name = title.trimmingCharacters(in: .whitespaces)
        do {
            let id = try model.core.createCommunity(title: name)
            model.refresh()
            Haptics.commit()
            // Signature reveal: doodle draws, wash blooms, then open the Space.
            revealing = (id, name)
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }
}

