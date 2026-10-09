import SwiftUI
import RodaCore

/// Mini-apps desenhados nativamente (o resto roda como View MCP no WKWebView).
enum NativeApps {
    static let ids: Set<String> = ["pet", "maptap", "recipe", "countdown", "hike"]
    static func isNative(_ app: String) -> Bool { ids.contains(app) }
}

/// Estado do mini-app decodificado do `view_json` do núcleo.
struct AppView {
    let raw: [String: Any]
    init(_ app: AppStateDto) {
        raw = (try? JSONSerialization.jsonObject(with: Data(app.viewJson.utf8))) as? [String: Any] ?? [:]
    }
    subscript(_ k: String) -> Any? { raw[k] }
    func string(_ k: String) -> String? { raw[k] as? String }
    func double(_ k: String) -> Double { (raw[k] as? NSNumber)?.doubleValue ?? 0 }
    func int(_ k: String) -> Int { (raw[k] as? NSNumber)?.intValue ?? 0 }
    func bool(_ k: String) -> Bool { (raw[k] as? NSNumber)?.boolValue ?? false }
    func array(_ k: String) -> [[String: Any]] { raw[k] as? [[String: Any]] ?? [] }
    func dict(_ k: String) -> [String: Any] { raw[k] as? [String: Any] ?? [:] }
    var log: [(who: String, what: String)] {
        array("log").map { (($0["who"] as? String) ?? "", ($0["what"] as? String) ?? "") }
    }
}

// MARK: - Zoen's marker in the chat (the mascot's head)

struct AgentOrb: View {
    var size: CGFloat = 26
    var tint: Color = Palette.action
    var body: some View {
        // Zoen's visual identity is the mascot: his furball head replaces the orb.
        MascotHead(size: size * 1.15)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

// MARK: - Saída do agente: widget quadrado ao vivo + linha de cartão

/// O que o agente posta quando cria um mini-app: o widget quadrado (prévia viva do
/// estado) e a linha de ação. Tocar abre a folha com mola (zoom); mudanças de qualquer
/// membro atualizam o widget para todos, com ponto vermelho e um coração subindo.
struct AppOutputBlock: View {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let card: ItemCard
    let app: AppStateDto

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button { open() } label: { AppWidgetSquare(title: card.title, app: app) }
                .buttonStyle(PressScaleStyle())
                .accessibilityLabel("\(card.title): \(app.headline)")
                .accessibilityHint("Opens the mini-app")
                .contextMenu {
                    let onHome = model.isOnHome(card.itemId)
                    Button(onHome ? "Unpin from Home" : "Pin to Home", systemImage: onHome ? "pin.slash" : "pin") {
                        onHome ? model.unpinFromHome(card.itemId) : model.pinToHome(card.itemId)
                    }
                }
            AppActionCard(itemId: card.itemId, title: card.title, app: app) { open() }
        }
        .appZoomSource(card.itemId, zoom)
    }

    private func open() {
        Haptics.open()
        model.openApp(card.itemId)
    }
}

struct AppWidgetSquare: View {
    let title: String
    let app: AppStateDto
    var side: CGFloat = 150

    var body: some View {
        // One renderer: the chat card is the same snapshot as the Home strip and widgets.
        if let snap = WidgetSnapshot.decode(app.snapshotJson) {
            SnapshotCard(snap: snap, side: side, corner: 26)
                .animation(.spring(duration: 0.5, bounce: 0.3), value: snap)
        } else {
            legacy
        }
    }

    private var legacy: some View {
        let v = AppView(app)
        return ZStack(alignment: .topLeading) {
            switch app.appId {
            case "pet":
                PetBackdrop(asleep: v.bool("asleep"))
                PetSprite(asleep: v.bool("asleep"), faded: v.bool("released"), bounce: true)
                    .frame(width: side * 0.62, height: side * 0.62)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(.top, 14)
            case "maptap":
                Color.black
                GlobeView(spin: true, interactive: false)
                    .padding(10)
            case "recipe":
                LinearGradient(colors: [Color(hex: "#FBE3C8"), Color(hex: "#F4B98A")], startPoint: .top, endPoint: .bottom)
                DoodleView(doodle: .pot).frame(width: side * 0.62, height: side * 0.62)
                    .frame(maxWidth: .infinity, maxHeight: .infinity).padding(.top, 22)
            default:
                LinearGradient(colors: [McpHost.tint(for: app.appId).opacity(0.25), InkPalette.paper], startPoint: .top, endPoint: .bottom)
                VStack(alignment: .leading, spacing: 4) {
                    Spacer()
                    DoodleView(doodle: app.appId == "poll" ? .ballot : .notepad)
                        .frame(width: side * 0.48, height: side * 0.48)
                        .frame(maxWidth: .infinity)
                    Text(app.headline).font(.caption2.weight(.semibold)).foregroundStyle(Color(hex: "#3A3A3C")).lineLimit(1)
                        .padding(.trailing, 14)
                }
                .padding(12)
            }
            Text(title)
                .font(.subheadline.weight(.bold))
                .foregroundStyle(app.appId == "maptap" || (app.appId == "pet" && v.bool("asleep")) ? .white : Color(hex: "#1C1C1E"))
                .lineLimit(2)
                .padding(12)
                .contentTransition(.numericText())
            Image(systemName: "arrow.up.right")
                .font(.caption2.weight(.bold))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)
                .padding(10)
        }
        .frame(width: side, height: side)
        .clipShape(.rect(cornerRadius: 26, style: .continuous))
        .shadow(color: .black.opacity(0.08), radius: 12, y: 6)
        .animation(.spring(duration: 0.5, bounce: 0.3), value: title)
    }
}

/// Linha de ação do Wabi: ícone em retângulo arredondado, título, espaço, seta.
struct AppActionCard: View {
    @Environment(AppModel.self) private var model
    let itemId: String
    let title: String
    let app: AppStateDto
    var onOpen: () -> Void
    @State private var hearts: [UUID] = []

    var body: some View {
        let unseen = model.unseenAppChange(itemId)
        Button(action: onOpen) {
            HStack(spacing: 12) {
                AppIcon(app: app, size: 38)
                Text(title).font(.body.weight(.semibold)).foregroundStyle(Color(hex: "#1C1C1E")).lineLimit(1)
                    .contentTransition(.numericText())
                Spacer(minLength: 8)
                Image(systemName: "arrow.right").font(.subheadline.weight(.semibold)).foregroundStyle(Color(hex: "#8E8E93"))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 9)
            .frame(maxWidth: 260)
            .background(.white, in: .rect(cornerRadius: 16, style: .continuous))
            .shadow(color: .black.opacity(0.05), radius: 8, y: 4)
            .overlay(alignment: .topTrailing) {
                if unseen {
                    Circle().fill(Color(hex: "#FF3B30")).frame(width: 10, height: 10)
                        .overlay(Circle().strokeBorder(.white, lineWidth: 2))
                        .offset(x: 3, y: -3)
                        .transition(.scale.combined(with: .opacity))
                }
            }
            .overlay(alignment: .topTrailing) {
                ZStack {
                    ForEach(hearts, id: \.self) { id in FloatingHeart().id(id) }
                }
                .offset(x: -18, y: -6)
                .allowsHitTesting(false)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(title)\(unseen ? ", novidade" : "")")
        .onChange(of: app.lastAction) { _, _ in
            guard unseen else { return }
            let id = UUID()
            hearts.append(id)
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(1.6))
                hearts.removeAll { $0 == id }
            }
        }
        .animation(.spring(duration: 0.35), value: unseen)
    }
}

struct FloatingHeart: View {
    @State private var up = false
    var body: some View {
        Image(systemName: "heart.fill")
            .font(.system(size: 18))
            .foregroundStyle(Color(hex: "#FF3B5C"))
            .offset(y: up ? -46 : 0)
            .scaleEffect(up ? 1.15 : 0.4)
            .opacity(up ? 0 : 1)
            .onAppear { withAnimation(.easeOut(duration: 1.4)) { up = true } }
    }
}

struct AppIcon: View {
    let app: AppStateDto
    var size: CGFloat = 38
    var body: some View {
        let v = AppView(app)
        ZStack {
            switch app.appId {
            case "pet":
                PetBackdrop(asleep: v.bool("asleep"))
                PetSprite(asleep: v.bool("asleep"), faded: v.bool("released"), bounce: false).padding(size * 0.12)
            case "maptap":
                Color.black
                GlobeView(spin: false, interactive: false).padding(size * 0.08)
            case "recipe":
                Color(hex: "#FBE3C8")
                DoodleView(doodle: .pot).padding(size * 0.06)
            case "countdown":
                LinearGradient(colors: [Color(hex: "#F9B67A"), Color(hex: "#3B6C8F")], startPoint: .top, endPoint: .bottom)
                DoodleView(doodle: .trip, freezeAt: 4).padding(size * 0.04)
            case "hike":
                Image(HikeInfo(app).photo).resizable().scaledToFill()
            default:
                Rectangle().fill(McpHost.tint(for: app.appId).gradient)
                Image(systemName: McpHost.symbol(for: app.appId)).font(.system(size: size * 0.42, weight: .semibold)).foregroundStyle(.white)
            }
        }
        .frame(width: size, height: size)
        .clipShape(.rect(cornerRadius: size * 0.28, style: .continuous))
    }
}

// MARK: - Folha do mini-app

struct AppSheetRef: Identifiable, Hashable { let id: String }

/// A folha que abre com mola a partir do cartão. Nativo para pet/MapTap/receita; View
/// MCP isolada (WKWebView) para os outros.
struct AppSheetHost: View {
    @Environment(AppModel.self) private var model
    @Environment(\.appZoom) private var zoom
    let itemId: String

    var body: some View {
        @Bindable var model = model
        let _ = model.revision
        Group {
            if let item = try? model.core.item(itemId: itemId), let app = item.app {
                switch app.appId {
                case "pet": PetSheet(item: item, app: app)
                case "maptap": MapTapSheet(item: item, app: app)
                case "recipe": RecipeSheet(item: item, app: app)
                case "countdown": CountdownSheet(item: item, app: app)
                case "hike": HikeSheet(item: item)
                default: McpAppSheet(item: item, app: app)
                }
            } else {
                InkEmptyState(pose: .roar, title: String(localized: "Mini-app unavailable"))
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("miniapp-sheet")
        .appZoomDestination(itemId, zoom)
        .presentationDragIndicator(.visible)
        .onAppear { model.markAppSeen(itemId) }
        .onChange(of: model.revision) { _, _ in model.markAppSeen(itemId) }
        .sheet(item: $model.appConfirm) { req in
            AppConfirmSheet(request: req) { model.appConfirm = nil }
        }
        .background {
            Color.clear.sheet(item: $model.consent) { req in
                ZoenConsentSheet(request: req) { model.consent = nil }
            }
        }
        #if os(macOS)
        .frame(minWidth: 420, idealWidth: 440, minHeight: 720, idealHeight: 780)
        #endif
    }
}

/// View MCP em folha (enquete, lista): o caminho do WKWebView isolado.
struct McpAppSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.miniAppClose) private var miniAppClose
    let item: ItemDetail
    let app: AppStateDto
    @State private var height: CGFloat = 400

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 12) {
                    McpAppWebView(itemId: item.id, displayMode: "fullscreen", height: $height)
                        .frame(height: max(height, 320))
                    HStack(spacing: 5) {
                        Image(systemName: "lock.shield")
                        Text("Sandboxed MCP View · no network · acts only in this mini-app")
                    }
                    .font(.caption2)
                    .foregroundStyle(Palette.textTertiary)
                }
                .padding(.horizontal, 8)
                .padding(.top, 8)
            }
            .background(Palette.background)
            .navigationTitle(item.title)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button { (miniAppClose ?? { dismiss() })() } label: { Image(systemName: "chevron.down") }.accessibilityLabel("Close").accessibilityIdentifier("miniapp-close")
                }
            }
        }
    }
}

/// Pílulas do Wabi: primária preta, secundária branca com contorno cinza.
struct WabiPill: ButtonStyle {
    var primary = false
    var dark = false
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(primary ? .white : (dark ? .white : Color(hex: "#1C1C1E")))
            .frame(maxWidth: .infinity, minHeight: 46)
            .background {
                Capsule().fill(primary ? Color(hex: "#111113") : (dark ? Color.white.opacity(0.14) : Color.white))
                    .shadow(color: .black.opacity(primary ? 0 : 0.06), radius: 8, y: 3)
            }
            .overlay { if !primary { Capsule().strokeBorder(dark ? .white.opacity(0.18) : Color.black.opacity(0.06), lineWidth: 0.8) } }
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .animation(.spring(duration: 0.25), value: configuration.isPressed)
    }
}
