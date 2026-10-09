import SwiftUI

/// "Escolher desenho": pick a hand-drawn AvatarV1 picture for a group/Space or an agent.
/// Stored on this device (`RodaGroupAvatar.<id>` / `RodaAgentAvatar.<id>`). "Gerar" comes with the art service.
struct AvatarArtPicker: View {
    enum Target: Identifiable, Hashable {
        case group(id: String, title: String)
        case agent(id: String, handle: String, name: String)
        var id: String {
            switch self {
            case .group(let id, _): "g-\(id)"
            case .agent(let id, _, _): "a-\(id)"
            }
        }
    }

    let target: Target
    @Environment(\.dismiss) private var dismiss
    @AppStorage("RodaAvatarArtVersion") private var artVersion = 0

    private var pool: [HandDrawnAvatarAsset.Asset] {
        switch target {
        case .group: HandDrawnAvatarAsset.groups
        case .agent: HandDrawnAvatarAsset.agents
        }
    }

    private var current: HandDrawnAvatarAsset.Asset? {
        switch target {
        case .group(let id, let title): HandDrawnAvatarAsset.pickGroup(spaceId: id, title: title)
        case .agent(let id, let handle, _): HandDrawnAvatarAsset.pickAgent(handle: handle, id: id)
        }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                let cur = current
                VStack(spacing: 18) {
                    if let cur {
                        HandDrawnAvatarView(asset: cur, size: 120)
                            .id("\(cur.id)-\(artVersion)")
                            .padding(.top, 8)
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 72), spacing: 14)], spacing: 14) {
                        ForEach(pool) { a in
                            Button {
                                choose(a)
                            } label: {
                                HandDrawnAvatarView(asset: a, size: 68, animate: false)
                                    .overlay {
                                        Circle().strokeBorder(a.id == cur?.id ? Palette.action : .clear, lineWidth: 3)
                                    }
                                    .padding(3)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(a.name)
                            .accessibilityIdentifier("art-\(a.id)")
                            .accessibilityAddTraits(a.id == cur?.id ? .isSelected : [])
                        }
                    }
                    .padding(.horizontal, 16)
                }
                .padding(.bottom, 24)
            }
            .navigationTitle(String(localized: "Choose drawing"))
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
        }
        .zoenSheet([.medium, .large])
    }

    private func choose(_ a: HandDrawnAvatarAsset.Asset) {
        Haptics.tap()
        switch target {
        case .group(let id, _): HandDrawnAvatarAsset.saveGroup(id, assetName: a.name)
        case .agent(let id, _, _): HandDrawnAvatarAsset.saveAgent(id, assetName: a.name)
        }
        artVersion &+= 1
    }
}
