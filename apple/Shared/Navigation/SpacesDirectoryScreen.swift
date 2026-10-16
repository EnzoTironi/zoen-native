import SwiftUI
import RodaCore

/// The directory is sourced from actual authorized spaces, never the selected demo chat.
struct SpacesDirectoryScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            ForEach(model.spaces, id: \.id) { space in
                Button { model.go(.space(space.id)) } label: {
                    HStack(spacing: 12) {
                        Image(systemName: "person.2")
                            .foregroundStyle(Palette.action)
                            .frame(width: 36, height: 36)
                            .background(Palette.action.opacity(0.1), in: .rect(cornerRadius: 10))
                        Text(space.title).foregroundStyle(Palette.textPrimary)
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption).foregroundStyle(Palette.textTertiary)
                    }
                    .padding(.vertical, 4)
                }
                .buttonStyle(.plain)
            }
        }
        .navigationTitle("Spaces")
        .accessibilityIdentifier("spaces-directory")
    }
}
