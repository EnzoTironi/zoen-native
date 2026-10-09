import SwiftUI
import RodaCore

/// Opens as a native sheet (medium → large). One entry point for every avatar/name tap.
struct ProfileSheetRef: Identifiable, Hashable {
    let id: String  // persona id
    var sourceID: String { "profile-\(id)" }
}

struct ProfileSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.appZoom) private var zoom
    let personaId: String
    @State private var detent: PresentationDetent = .medium
    @State private var editing = false
    @State private var artPicker: AvatarArtPicker.Target?

    private var persona: Persona? {
        if model.me?.id == personaId { return model.me }
        return model.core.personas().first { $0.id == personaId }
            ?? model.spaces.flatMap(\.members).first { $0.id == personaId }
            ?? model.agents.first { $0.persona.id == personaId }?.persona
    }

    var body: some View {
        NavigationStack {
            Group {
                if let p = persona {
                    content(p)
                } else {
                    ContentUnavailableView(String(localized: "Person not found"), systemImage: "person.crop.circle.badge.questionmark")
                }
            }
            .background(InkPalette.paper.opacity(0.35).ignoresSafeArea())
        }
        .presentationDetents([.medium, .large], selection: $detent)
        .presentationDragIndicator(.visible)
        .presentationBackground {
            // iOS 26 Liquid Glass sheet surface
            Rectangle().fill(.clear).glassEffect(.regular, in: .rect(cornerRadius: 28, style: .continuous))
        }
        .modifier(ProfileZoomDestination(id: "profile-\(personaId)", ns: zoom))
    }

    @ViewBuilder
    private func content(_ p: Persona) -> some View {
        ScrollView {
            VStack(spacing: 18) {
                hero(p)
                if p.kind == .agent && p.handle != "zoen" {
                    Button {
                        Haptics.tap()
                        artPicker = .agent(id: p.id, handle: p.handle, name: p.name)
                    } label: {
                        Label(String(localized: "Choose drawing"), systemImage: "paintbrush.pointed")
                            .font(.subheadline.weight(.semibold))
                    }
                    .buttonStyle(.glass)
                }
                actions(p)
                if detent == .large || true {
                    shared(p)
                    danger(p)
                }
            }
            .padding(.horizontal, 20)
            .padding(.top, 8)
            .padding(.bottom, 28)
        }
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            if p.isMe {
                ToolbarItem(placement: .primaryAction) {
                    Button(String(localized: "Edit")) { editing = true }
                }
            }
            ToolbarItem(placement: .cancellationAction) {
                Button(String(localized: "Done")) { dismiss() }
            }
        }
        .sheet(isPresented: $editing) {
            if let me = model.me { ProfileEditorSheet(persona: me) }
        }
        .sheet(item: $artPicker) { target in
            AvatarArtPicker(target: target)
        }
    }

    private func hero(_ p: Persona) -> some View {
        VStack(spacing: 10) {
            Group {
                if p.kind == .agent {
                    // Zoen keeps the mascot; other agents get their AvatarV1 drawing (loops here).
                    ContactAvatar(persona: p, size: 104)
                } else {
                    PersonAvatar(persona: p, size: 104)
                }
            }
            .accessibilityHidden(true)

            Text(p.name)
                .font(.title2.weight(.bold))
                .foregroundStyle(Palette.textPrimary)
            Text(verbatim: "@\(p.handle)")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(Palette.textSecondary)
            let bio = p.bio.trimmingCharacters(in: .whitespacesAndNewlines)
            if !bio.isEmpty {
                Text(bio)
                    .font(.body)
                    .foregroundStyle(Palette.textSecondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 8)
            } else if p.kind == .agent, let owner = p.ownerName {
                Text(String(localized: "Added by \(owner)"))
                    .font(.subheadline)
                    .foregroundStyle(Palette.textTertiary)
            }
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(p.name), @\(p.handle)")
    }

    private func actions(_ p: Persona) -> some View {
        HStack(spacing: 10) {
            glassAction(String(localized: "Message"), symbol: "bubble.left.fill") {
                dismiss()
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                    model.openChat(with: p)
                }
            }
            glassAction(String(localized: "Call"), symbol: "phone.fill") {
                model.show(.init(kind: .info, text: String(localized: "Voice calls arrive in a later build.")))
            }
            glassAction(String(localized: "Video"), symbol: "video.fill") {
                model.show(.init(kind: .info, text: String(localized: "Video calls arrive in a later build.")))
            }
        }
    }

    private func glassAction(_ title: String, symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: { Haptics.tap(); action() }) {
            VStack(spacing: 6) {
                Image(systemName: symbol).font(.system(size: 18, weight: .semibold))
                Text(title).font(.caption.weight(.semibold))
            }
            .foregroundStyle(Palette.action)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
    }

    @ViewBuilder
    private func shared(_ p: Persona) -> some View {
        let spaces = model.spaces.filter { $0.members.contains { $0.id == p.id } && $0.counterpart == nil }
        if !spaces.isEmpty {
            section(String(localized: "Spaces in common")) {
                ForEach(spaces.prefix(6)) { s in
                    Button {
                        dismiss()
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { model.go(.space(s.id)) }
                    } label: {
                        HStack(spacing: 12) {
                            ChatAvatar(space: s, size: 40)
                            Text(s.title).foregroundStyle(Palette.textPrimary)
                            Spacer()
                            ZoenIcon(.chevron, size: 12).foregroundStyle(Palette.textTertiary)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }

        if p.kind == .agent {
            section(String(localized: "In this chat")) {
                if let sid = model.paths[model.tab]?.compactMap({ if case .space(let id) = $0 { return id }; if case .participants(let id) = $0 { return id }; return nil }).last
                    ?? model.spaces.first(where: { $0.members.contains { $0.id == p.id } })?.id {
                    Button {
                        dismiss()
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                            model.go(.permissions(agent: p.id, space: sid))
                        }
                    } label: {
                        Label(String(localized: "Permissions"), systemImage: "slider.horizontal.3")
                            .foregroundStyle(Palette.textPrimary)
                    }
                }
                if let owner = p.ownerName {
                    LabeledContent(String(localized: "Added by"), value: owner)
                }
            }
        }

        if p.kind == .agent && p.isMine {
            let decisions = model.standing(for: p.id)
            if !decisions.isEmpty {
                section(String(localized: "Standing decisions")) {
                    VStack(spacing: 12) {
                        ForEach(decisions, id: \.grantId) { d in StandingDecisionRow(decision: d) }
                    }
                }
            }
        }

        // Shared media/files placeholder from local store (no jargon)
        section(String(localized: "Shared")) {
            Text(String(localized: "Photos, links and files you share in chats appear here."))
                .font(.subheadline)
                .foregroundStyle(Palette.textSecondary)
        }
    }

    private func danger(_ p: Persona) -> some View {
        Group {
            if !p.isMe && p.kind == .person {
                section(nil) {
                    // Each one turns into its own confirmation, right where you tapped.
                    VStack(spacing: 4) {
                        ConfirmInPlaceButton(title: String(localized: "Mute"), systemImage: "bell.slash",
                                             confirmTitle: String(localized: "Mute \(p.name)?"),
                                             doneTitle: String(localized: "Muted"), identifier: "profile-mute") {
                            model.show(.init(kind: .info, text: String(localized: "Muted on this device.")))
                        }
                        ConfirmInPlaceButton(title: String(localized: "Block"), systemImage: "hand.raised",
                                             confirmTitle: String(localized: "Block \(p.name)? They won’t know"),
                                             doneTitle: String(localized: "Blocked"), identifier: "profile-block") {
                            model.show(.init(kind: .info, text: String(localized: "Blocked on this device. Syncing block lists comes later.")))
                            dismiss()
                        }
                        ConfirmInPlaceButton(title: String(localized: "Report"), systemImage: "exclamationmark.bubble",
                                             confirmTitle: String(localized: "Report \(p.name)?"),
                                             doneTitle: String(localized: "Reported"), identifier: "profile-report") {
                            model.show(.init(kind: .info, text: String(localized: "Thanks — we’ll look into it.")))
                            dismiss()
                        }
                    }
                }
            }
        }
    }

    private func section<Content: View>(_ title: String?, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let title {
                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textSecondary)
            }
            VStack(spacing: 0) { content() }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassEffect(.regular, in: .rect(cornerRadius: 18, style: .continuous))
        }
    }
}

/// Own profile editor (photo + name + bio). Handle stays in Account settings.
private struct ProfileEditorSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let persona: Persona
    @State private var name: String = ""
    @State private var bio: String = ""
    @State private var pending: PlatformImage?
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    AvatarPhotoPicker(personaId: persona.id, pending: $pending, size: 96,
                                      initials: persona.initials, tintHex: persona.tintHex)
                        .frame(maxWidth: .infinity)
                        .listRowBackground(Color.clear)
                    TextField(String(localized: "Your name"), text: $name)
                    TextField(String(localized: "Bio"), text: $bio, axis: .vertical)
                        .lineLimit(2...4)
                }
                if let failure {
                    Section { Text(failure).foregroundStyle(Palette.danger) }
                }
            }
            .navigationTitle(String(localized: "Edit profile"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(String(localized: "Cancel")) { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Save")) { save() }
                        .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .onAppear { name = persona.name; bio = persona.bio }
        }
        .presentationDetents([.medium, .large])
    }

    private func save() {
        do {
            try model.sync.updateProfile(name: name, handle: model.sync.account?.handle ?? persona.handle, bio: bio)
            if let img = pending { AvatarPhotoStore.save(persona.id, image: img) }
            model.refresh()
            Haptics.commit()
            dismiss()
        } catch {
            failure = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }
}

private struct ProfileZoomDestination: ViewModifier {
    let id: String
    let ns: Namespace.ID?
    func body(content: Content) -> some View {
        #if os(iOS)
        if let ns {
            content.navigationTransition(.zoom(sourceID: id, in: ns))
        } else {
            content
        }
        #else
        content
        #endif
    }
}

// MARK: - Shared entry point

extension View {
    /// Tap opens the profile sheet. New surfaces get it for free.
    func profileLink(_ persona: Persona?, enabled: Bool = true) -> some View {
        modifier(ProfileLinkModifier(persona: persona, enabled: enabled))
    }

    func profileLink(id: String?, enabled: Bool = true) -> some View {
        modifier(ProfileLinkIdModifier(id: id, enabled: enabled))
    }
}

private struct ProfileLinkModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let persona: Persona?
    var enabled: Bool
    func body(content: Content) -> some View {
        if let persona, enabled, !persona.isMe || true {
            content
                .contentShape(.rect)
                .profileZoomSource("profile-\(persona.id)", zoom)
                .onTapGesture {
                    Haptics.tap()
                    model.openProfile(persona.id)
                }
                .accessibilityAddTraits(.isButton)
                .accessibilityHint(String(localized: "Shows profile"))
        } else {
            content
        }
    }
}

private struct ProfileLinkIdModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let id: String?
    var enabled: Bool
    func body(content: Content) -> some View {
        if let id, enabled {
            content
                .contentShape(.rect)
                .profileZoomSource("profile-\(id)", zoom)
                .onTapGesture {
                    Haptics.tap()
                    model.openProfile(id)
                }
                .accessibilityAddTraits(.isButton)
                .accessibilityHint(String(localized: "Shows profile"))
        } else {
            content
        }
    }
}

extension View {
    /// Zoom source for avatar/name taps. Unlike `appZoomSource` (26 pt rounded rect for app
    /// cards), no clip: a 26 pt continuous corner on a 30 pt face or a caption line turned
    /// monograms into diamonds and short names into tiny glyphs.
    @ViewBuilder
    func profileZoomSource(_ id: String, _ ns: Namespace.ID?) -> some View {
        #if os(iOS)
        if let ns { self.matchedTransitionSource(id: id, in: ns) } else { self }
        #else
        self
        #endif
    }
}
