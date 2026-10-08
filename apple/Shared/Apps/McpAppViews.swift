import SwiftUI
import RodaCore

extension EnvironmentValues {
    /// Namespace da transição zoom cartão → tela cheia (iPhone).
    @Entry var appZoom: Namespace.ID? = nil
}

extension View {
    @ViewBuilder
    func appZoomSource(_ id: String, _ ns: Namespace.ID?) -> some View {
        #if os(iOS)
        if let ns { self.matchedTransitionSource(id: id, in: ns) { $0.clipShape(.rect(cornerRadius: 26)) } } else { self }
        #else
        self
        #endif
    }

    @ViewBuilder
    func appZoomDestination(_ id: String, _ ns: Namespace.ID?) -> some View {
        #if os(iOS)
        if let ns { self.navigationTransition(.zoom(sourceID: id, in: ns)) } else { self }
        #else
        self
        #endif
    }
}

// MARK: - Cartão na conversa

/// O mini-app vivo dentro da conversa: moldura de vidro, cabeçalho nativo (quem, versão,
/// última ação, expandir) e o View MCP isolado no meio. Tocar em expandir abre em tela
/// cheia com zoom; arrastar para baixo volta.
struct AppCardView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let card: ItemCard
    let app: AppStateDto
    @State private var height: CGFloat = 250

    var body: some View {
        let tint = McpHost.tint(for: app.appId)
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: McpHost.symbol(for: app.appId))
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
                    .background(tint.gradient, in: .rect(cornerRadius: 9, style: .continuous))
                VStack(alignment: .leading, spacing: 0) {
                    Text(card.title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary).lineLimit(1)
                    Text("Mini-app · v\(card.version)\(app.lastAction.map { " · \($0)" } ?? "")")
                        .font(.caption2).foregroundStyle(Palette.textSecondary).lineLimit(1)
                        .contentTransition(.opacity)
                }
                Spacer(minLength: 4)
                Button { open() } label: {
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .font(.system(size: 13, weight: .bold))
                        .frame(width: 36, height: 36)
                }
                .buttonStyle(.plain)
                .glassEffect(.regular.interactive(), in: .circle)
                .accessibilityLabel("Open \(card.title) full screen")
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 4)

            McpAppWebView(itemId: card.itemId, displayMode: "inline", height: $height, onFullscreen: open)
                .frame(height: height)
                .accessibilityLabel("\(app.name): \(app.headline)")

            HStack(spacing: 5) {
                Image(systemName: "lock.shield").font(.caption2)
                Text("Sandboxed · no network · acts only in this mini-app")
                Spacer()
            }
            .font(.caption2)
            .foregroundStyle(Palette.textTertiary)
            .padding(.horizontal, 14)
            .padding(.bottom, 10)
        }
        .background(Palette.surface.opacity(0.55), in: .rect(cornerRadius: 26, style: .continuous))
        .glassEffect(.regular.tint(tint.opacity(0.06)), in: .rect(cornerRadius: 26, style: .continuous))
        .appZoomSource(card.itemId, zoom)
        .frame(maxWidth: 420)
    }

    private func open() {
        Haptics.open()
        model.openApp(card.itemId)
    }
}

// MARK: - Tela cheia

struct AppFullScreenView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let itemId: String
    @State private var height: CGFloat = 600
    @State private var item: ItemDetail?

    var body: some View {
        ScrollView {
            VStack(spacing: 14) {
                if let app = item?.app {
                    HStack(spacing: 8) {
                        ForEach(app.metrics, id: \.label) { m in
                            MetricPill(label: m.label, value: m.value, tint: McpHost.tint(for: app.appId))
                        }
                    }
                    .padding(.horizontal, 16)
                }
                McpAppWebView(itemId: itemId, displayMode: "fullscreen", height: $height)
                    .frame(height: max(height, 420))
                    .background(Palette.surface, in: .rect(cornerRadius: 28, style: .continuous))
                    .padding(.horizontal, 12)
                if let item {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("History").font(.headline)
                        ForEach(item.versions.reversed().prefix(6), id: \.number) { v in
                            HStack(spacing: 8) {
                                Avatar(persona: v.author, size: 22)
                                Text(v.note).font(.subheadline).lineLimit(1)
                                Spacer()
                                Text("v\(v.number)").font(.caption.monospacedDigit()).foregroundStyle(Palette.textTertiary)
                            }
                        }
                        Text("Every tap is a signed version in the core. Everyone in the Space sees the same state.")
                            .font(.caption).foregroundStyle(Palette.textSecondary)
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Palette.surface, in: .rect(cornerRadius: 22, style: .continuous))
                    .padding(.horizontal, 12)
                }
            }
            .padding(.vertical, 12)
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle(item?.title ?? "Mini-app")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .task(id: model.revision) { item = try? model.core.item(itemId: itemId) }
    }
}

struct MetricPill: View {
    let label: String
    let value: Double
    let tint: Color
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(label).font(.caption2.weight(.semibold)).foregroundStyle(Palette.textSecondary).lineLimit(1)
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.surfaceMuted)
                    Capsule().fill(tint.gradient).frame(width: max(6, g.size.width * value))
                }
            }
            .frame(height: 6)
        }
        .padding(10)
        .frame(maxWidth: .infinity)
        .background(Palette.surface, in: .rect(cornerRadius: 14, style: .continuous))
        .animation(.spring(duration: 0.5, bounce: 0.3), value: value)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label): \(Int((value * 100).rounded())) percent")
    }
}

// MARK: - Confirmação nativa

/// Folha nativa para ações irreversíveis ou que saem do Espaço, pedidas por um mini-app.
/// O mini-app nunca vê nem desenha este botão: quem confirma é o host.
struct AppConfirmSheet: View {
    let request: AppConfirmRequest
    var onDone: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: request.destructive ? "exclamationmark.shield.fill" : "arrow.up.forward.app.fill")
                .font(.system(size: 34, weight: .semibold))
                .foregroundStyle(request.destructive ? Palette.amber : Palette.action)
                .padding(.top, 8)
            VStack(spacing: 6) {
                Text(request.title).font(.title3.weight(.bold)).multilineTextAlignment(.center)
                Text(request.detail).font(.subheadline).foregroundStyle(Palette.textSecondary).multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Text(request.confirmLabel == nil ? String(localized: "Requested by \(request.appName) · this isn’t covered yet, so only you decide.") : String(localized: "Requested by \(request.appName) · only you decide."))
                .font(.caption).foregroundStyle(Palette.textTertiary).multilineTextAlignment(.center)
            VStack(spacing: 10) {
                Button(role: request.destructive ? .destructive : nil) {
                    Haptics.commit(); request.decide(true); onDone()
                } label: {
                    Text(request.confirmLabel ?? String(localized: "Confirm")).font(.headline).frame(maxWidth: .infinity).frame(height: 50)
                }
                .buttonStyle(.glassProminent)
                .tint(request.destructive ? Palette.danger : Palette.action)
                Button {
                    Haptics.dismiss(); request.decide(false); onDone()
                } label: {
                    Text("Not now").font(.headline).frame(maxWidth: .infinity).frame(height: 50)
                }
                .buttonStyle(.glass)
            }
        }
        .padding(.horizontal, 22)
        .padding(.bottom, 12)
        .presentationDetents([.height(400), .large])
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled()
    }
}
