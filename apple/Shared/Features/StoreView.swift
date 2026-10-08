import SwiftUI
import RodaCore

// MARK: - Store (the marketplace): agents and mini-apps
//
// Layout follows Wabi's Explore: header (bell + avatar), Collections, Featured today,
// Invites, Popular this week (chips + ranked list) and By people you follow.
// Zoen twist: agents are listed like contacts (round avatar, @creator, Message), mini-apps
// keep rounded-square icons. Content art is pen and paper (hand-drawn ink doodles with
// marker washes on procedural paper); the chrome stays Liquid Glass.
// Everything below is SEED DATA for the prototype: example listings, not real creators.

struct StoreListing: Identifiable, Hashable {
    enum Kind: Hashable { case agent, miniApp }
    enum Category: String, CaseIterable, Hashable { case lifestyle, health, games, productivity, music }

    let id: String
    let kind: Kind
    let name: String
    var subtitle: String? = nil
    let blurb: String
    let about: String
    let creator: String
    let doodle: Doodle
    let accentHex: String
    let category: Category
    let uses: String
    let rating: String
    let community: String
    /// For agents: the contact handle to open a chat with, when it exists on this device.
    var handle: String? = nil

    var accent: Color { Color(hex: accentHex) }
    var isAgent: Bool { kind == .agent }
}

struct StoreCollection: Identifiable {
    let id: String
    let title: String
    let subtitle: String
    let doodle: Doodle
    let wash: Color
    let items: [String]
}

/// Seed data, English and pt-BR. Clearly example content (see the footnote on screen).
@MainActor
enum StoreSeed {
    private static func p(_ pt: String, _ en: String) -> String { AppLocale.pick(pt, en) }

    static var listings: [StoreListing] { [
        StoreListing(id: "vinyl", kind: .miniApp, name: "Vinyl Cover Flow",
                     blurb: p("Os discos da galera num cover flow desenhado à mão.", "Your group's records in a hand-drawn cover flow."),
                     about: p("Cada um adiciona os discos que está ouvindo; o grupo vota no da semana e o vencedor gira no topo do chat.", "Everyone adds what they're spinning; the group votes on the record of the week and the winner spins at the top of the chat."),
                     creator: "joonas", doodle: .vinyl, accentHex: "#17808A", category: .music, uses: "12k", rating: "4.8", community: p("Clube do Vinil · 2,1 mil membros", "Vinyl Club · 2.1k members")),
        StoreListing(id: "moss", kind: .agent, name: "Moss", subtitle: p("O espírito do jardim", "The Garden Spirit"),
                     blurb: p("Um companheiro gentil que lembra o grupo de regar as plantas.", "A gentle companion that reminds the group to water the plants."),
                     about: p("Moss conhece cada planta da casa, avisa quando é a vez de alguém regar e comemora folhas novas com o grupo.", "Moss knows every plant in the house, nudges whoever's turn it is to water, and celebrates new leaves with the group."),
                     creator: "tolya", doodle: .moss, accentHex: "#3D8A3A", category: .lifestyle, uses: "8.4k", rating: "4.9", community: p("Plantinhas · 940 membros", "Plant People · 940 members")),
        StoreListing(id: "reset", kind: .miniApp, name: p("Perguntas pra recomeçar", "Life reset questions"),
                     blurb: p("Um roteiro de um dia pra reflexão, a sós ou em dupla.", "A one-day framework for self-reflection, solo or as a pair."),
                     about: p("Doze perguntas ao longo do dia, com respostas privadas por padrão. No fim, um resumo só seu.", "Twelve prompts across the day, answers private by default. At the end, a summary just for you."),
                     creator: "anna", doodle: .hourglass, accentHex: "#2F6FB0", category: .productivity, uses: "21k", rating: "4.7", community: p("Diários · 3,4 mil membros", "Journaling · 3.4k members")),
        StoreListing(id: "trail", kind: .agent, name: "Trail Buddy",
                     blurb: p("Acha trilhas, olha o tempo e organiza as caronas.", "Finds trails, checks the weather and sorts out rides."),
                     about: p("Peça uma trilha no chat: Trail Buddy compara três opções com mapa e fotos, abre a votação e monta o roteiro com caronas.", "Ask for a hike in the chat: Trail Buddy compares three options with maps and photos, runs the vote and plans the day with rides."),
                     creator: "lucas.s", doodle: .hike, accentHex: "#B5532E", category: .lifestyle, uses: "5.2k", rating: "4.8", community: p("Trilheiros SP · 1,2 mil membros", "Weekend Hikers · 1.2k members")),
        StoreListing(id: "steps", kind: .miniApp, name: "Steps Grid",
                     blurb: p("A meta de passos do grupo num quadriculado de dias.", "The group's step goal as a grid of days."),
                     about: p("Cada quadradinho é um dia; ele se pinta quando alguém bate a meta. Dá pra puxar do app Saúde com sua permissão.", "Each square is a day and fills in when someone hits the goal. Pulls from Health with your permission."),
                     creator: "ryan_v", doodle: .steps, accentHex: "#2E8B57", category: .health, uses: "17k", rating: "4.6", community: p("Corre Junto · 2,8 mil membros", "Run Club · 2.8k members")),
        StoreListing(id: "zoen", kind: .agent, name: "Zoen",
                     blurb: p("Seu agente: planeja, cria mini-apps e cuida dos detalhes.", "Your agent: plans, builds mini-apps and handles the details."),
                     about: p("O Zoen vive nos seus chats como um contato. Pede permissão antes de agir fora do chat.", "Zoen lives in your chats like a contact. It asks before acting outside the chat."),
                     creator: "zoen", doodle: .roda, accentHex: "#3D7A28", category: .productivity, uses: "—", rating: "—", community: p("Comunidade Zoen", "Zoen community"), handle: "zoen"),
        StoreListing(id: "hike", kind: .miniApp, name: p("Trilha de sábado", "Saturday hike"),
                     blurb: p("Compare trilhas, votem e montem o dia com caronas.", "Compare trails, vote and plan the day with rides."),
                     about: p("Três trilhas com mapa e fotos, votação ao vivo e um roteiro com horários e caronas.", "Three trails with maps and photos, a live vote, and a plan with times and rides."),
                     creator: "zoen", doodle: .hike, accentHex: "#3D7A28", category: .lifestyle, uses: "31k", rating: "4.9", community: p("Trilheiros SP · 1,2 mil membros", "Weekend Hikers · 1.2k members")),
        StoreListing(id: "breath", kind: .miniApp, name: p("Respira", "Breathwork"),
                     blurb: p("Respiração guiada com timer, sozinho ou em grupo.", "Calming breathwork with timers, solo or together."),
                     about: p("Sessões de 1 a 10 minutos. Em grupo, todo mundo respira no mesmo ritmo.", "Sessions from 1 to 10 minutes. In a group, everyone breathes in sync."),
                     creator: "joonas", doodle: .heart, accentHex: "#C8463A", category: .health, uses: "14k", rating: "4.7", community: p("Calma · 1,9 mil membros", "Calm Corner · 1.9k members")),
        StoreListing(id: "list", kind: .miniApp, name: p("A lista", "The list"),
                     blurb: p("Salve e organize o que o grupo quer fazer.", "Save and organize what the group wants to do."),
                     about: p("Lugares, filmes, receitas: qualquer um adiciona, todo mundo marca.", "Places, films, recipes: anyone adds, everyone ticks."),
                     creator: "marina", doodle: .notepad, accentHex: "#B07A12", category: .productivity, uses: "9.8k", rating: "4.6", community: p("Listeiros · 610 membros", "List Makers · 610 members")),
        StoreListing(id: "pin", kind: .miniApp, name: "Map Tap",
                     blurb: p("Adivinhe o lugar no mapa. Rodada nova todo dia.", "Guess the place on the map. A new round every day."),
                     about: p("Uma foto, um mapa, um alfinete. O placar do grupo fica fixado no chat.", "One photo, one map, one pin. The group's scoreboard stays pinned in the chat."),
                     creator: "ana", doodle: .pin, accentHex: "#D0453A", category: .games, uses: "26k", rating: "4.8", community: p("Geo Nerds · 4,1 mil membros", "Geo Nerds · 4.1k members")),
        StoreListing(id: "dinner", kind: .miniApp, name: p("Troca de receitas", "Recipe swap"),
                     blurb: p("Cada um traz um prato; a lista de compras se monta sozinha.", "Everyone brings a dish; the shopping list builds itself."),
                     about: p("Escolham as receitas, dividam os ingredientes e cozinhem passo a passo.", "Pick recipes, split the ingredients and cook step by step."),
                     creator: "ana", doodle: .pot, accentHex: "#C25B2A", category: .lifestyle, uses: "7.1k", rating: "4.5", community: p("Cozinha de Domingo · 880 membros", "Sunday Kitchen · 880 members")),
    ] }

    static func listing(_ id: String) -> StoreListing? { listings.first { $0.id == id } }

    static var collections: [StoreCollection] { [
        StoreCollection(id: "body", title: p("Pra um corpo mais saudável", "For a healthier body"),
                        subtitle: p("Mini-apps e agentes da comunidade pra entrar em forma este ano.", "Top mini-apps and agents from the community to help you get in shape this year."),
                        doodle: .heart, wash: InkPalette.tomato, items: ["steps", "breath", "reset", "moss"]),
        StoreCollection(id: "habits", title: p("Pra hábitos melhores", "For better habits"),
                        subtitle: p("Pequenos rituais diários que o grupo sustenta junto.", "Small daily rituals the group keeps up together."),
                        doodle: .hourglass, wash: InkPalette.butter, items: ["reset", "list", "steps", "breath"]),
        StoreCollection(id: "plans", title: p("Planos com amigos", "Plans with friends"),
                        subtitle: p("Trilhas, viagens e jantares, decididos no chat.", "Hikes, trips and dinners, decided in the chat."),
                        doodle: .hike, wash: InkPalette.mint, items: ["hike", "trail", "dinner", "pin"]),
        StoreCollection(id: "music", title: p("Noites de música", "Music nights"),
                        subtitle: p("Discos, playlists e o álbum da semana.", "Records, playlists and the album of the week."),
                        doodle: .vinyl, wash: InkPalette.lilac, items: ["vinyl", "list", "pin", "moss"]),
        StoreCollection(id: "pets", title: p("Companheiros pequenos", "Little companions"),
                        subtitle: p("Agentes e bichinhos que vivem no chat do grupo.", "Agents and pets that live in the group chat."),
                        doodle: .moss, wash: InkPalette.mint, items: ["moss", "trail", "zoen", "vinyl"]),
        StoreCollection(id: "organized", title: p("Pra se organizar", "Get organized"),
                        subtitle: p("Listas, divisões e lembretes sem planilha.", "Lists, splits and reminders, no spreadsheet."),
                        doodle: .notepad, wash: InkPalette.sky, items: ["list", "reset", "dinner", "zoen"]),
    ] }

    static let featured = ["vinyl", "moss", "reset", "trail", "steps", "dinner"]
    static let popular = ["hike", "moss", "breath", "pin", "vinyl", "trail", "steps", "reset", "list", "dinner"]
    static let following: [(String, String)] = [("steps", "lucas"), ("list", "marina"), ("pin", "ana"), ("dinner", "ana")]
}

enum StoreFilter: String, CaseIterable, Identifiable {
    case top, agents, lifestyle, health, games, productivity, music
    var id: String { rawValue }
    @MainActor var label: String {
        switch self {
        case .top: AppLocale.pick("Top", "Top")
        case .agents: AppLocale.pick("Agentes", "Agents")
        case .lifestyle: AppLocale.pick("Estilo de vida", "Lifestyle")
        case .health: AppLocale.pick("Saúde e forma", "Health & Fitness")
        case .games: AppLocale.pick("Jogos", "Games")
        case .productivity: AppLocale.pick("Produtividade", "Productivity")
        case .music: AppLocale.pick("Música", "Music")
        }
    }
    var glyph: ZoenGlyph {
        switch self {
        case .top: .crown
        case .agents: .agents
        case .lifestyle: .sun
        case .health: .heart
        case .games: .game
        case .productivity: .list
        case .music: .note
        }
    }
    func matches(_ l: StoreListing) -> Bool {
        switch self {
        case .top: true
        case .agents: l.isAgent
        case .lifestyle: l.category == .lifestyle
        case .health: l.category == .health
        case .games: l.category == .games
        case .productivity: l.category == .productivity
        case .music: l.category == .music
        }
    }
}

// MARK: - Screen

struct StoreScreen: View {
    @Environment(AppModel.self) private var model
    @State private var filter: StoreFilter = .top
    @State private var pos = ScrollPosition(edge: .top)
    @State private var width: CGFloat = 402

    private var ranked: [StoreListing] {
        StoreSeed.popular.compactMap(StoreSeed.listing).filter(filter.matches).prefix(5).map { $0 }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                StoreHeader()
                    .padding(.horizontal, 20)
                    .padding(.top, 4)
                    .padding(.bottom, 18)

                sectionTitle(AppLocale.pick("Coleções", "Collections"))
                carousel(spacing: 14) {
                    ForEach(StoreSeed.collections) { c in
                        CollectionCard(collection: c, width: (width * 0.75).rounded())
                    }
                }
                .padding(.bottom, 28)

                sectionTitle(AppLocale.pick("Destaques de hoje", "Featured today"))
                carousel(spacing: 14) {
                    ForEach(StoreSeed.featured.compactMap(StoreSeed.listing)) { l in
                        NavigationLink(value: Route.store(l.id)) {
                            FeaturedCard(listing: l, width: (width * 0.62).rounded())
                        }
                        .buttonStyle(PressScaleStyle())
                    }
                }
                .padding(.bottom, 28)

                sectionTitle(AppLocale.pick("Convites", "Invites"))
                InviteCard().padding(.horizontal, 20).padding(.bottom, 30)

                sectionTitle(AppLocale.pick("Populares da semana", "Popular this week"), chevron: true)
                chips.padding(.bottom, 10)
                VStack(spacing: 4) {
                    ForEach(Array(ranked.enumerated()), id: \.element.id) { i, l in
                        NavigationLink(value: Route.store(l.id)) { RankedRow(rank: i + 1, listing: l) }
                            .buttonStyle(.plain)
                    }
                    if ranked.isEmpty {
                        Text(AppLocale.pick("Nada aqui ainda.", "Nothing here yet."))
                            .font(.subheadline).foregroundStyle(Palette.textSecondary)
                            .frame(maxWidth: .infinity, minHeight: 80)
                    }
                }
                .padding(.horizontal, 20)
                .animation(.snappy, value: filter)
                .padding(.bottom, 28)

                sectionTitle(AppLocale.pick("De quem você segue", "By people you follow"), chevron: true)
                carousel(spacing: 12) {
                    ForEach(StoreSeed.following, id: \.0) { id, by in
                        if let l = StoreSeed.listing(id) {
                            NavigationLink(value: Route.store(l.id)) {
                                FollowCard(listing: l, by: by, width: (width * 0.42).rounded())
                            }
                            .buttonStyle(PressScaleStyle())
                        }
                    }
                }
                .padding(.bottom, 22)

                Label { Text(AppLocale.pick("Dados de exemplo: vitrine do protótipo, sem criadores reais.", "Seed data: example listings for the prototype, not real creators.")) } icon: { ZoenIcon(.info, size: 14) }
                    .font(.caption)
                    .foregroundStyle(Palette.textTertiary)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 12)
            }
        }
        .scrollPosition($pos)
        .scrollIndicators(.hidden)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .background(Palette.background.ignoresSafeArea())
        #if os(iOS)
        .toolbar(.hidden, for: .navigationBar)
        #endif
        .task { await screenshotScroll() }
    }

    /// Screenshots and the scroll video: `-RodaStoreScroll 640` jumps there;
    /// `-RodaStoreAutoScroll YES` glides down the page and back.
    private func screenshotScroll() async {
        let d = UserDefaults.standard
        if let id = d.string(forKey: "RodaStoreDetail") {
            try? await Task.sleep(for: .seconds(0.8))
            model.push(.store(id))
        }
        let y = d.double(forKey: "RodaStoreScroll")
        if y > 0 {
            try? await Task.sleep(for: .seconds(1.2))
            withAnimation(.smooth(duration: 0.5)) { pos.scrollTo(y: y) }
        }
        if d.bool(forKey: "RodaStoreAutoScroll") {
            try? await Task.sleep(for: .seconds(2.5))
            for target in [360.0, 760, 1180, 1700, 2100, 0] {
                withAnimation(.easeInOut(duration: 1.6)) { pos.scrollTo(y: target) }
                try? await Task.sleep(for: .seconds(2.1))
            }
        }
    }

    private func sectionTitle(_ s: String, chevron: Bool = false) -> some View {
        HStack {
            Text(s).font(.system(size: 21, weight: .bold)).foregroundStyle(Palette.textPrimary)
            Spacer()
            if chevron {
                ZoenIcon(.chevron, size: 15).foregroundStyle(Palette.textSecondary)
            }
        }
        .accessibilityAddTraits(.isHeader)
        .padding(.horizontal, 20)
        .padding(.bottom, 12)
    }

    private func carousel<C: View>(spacing: CGFloat, @ViewBuilder _ content: () -> C) -> some View {
        ScrollView(.horizontal) {
            LazyHStack(alignment: .top, spacing: spacing) { content() }
                .scrollTargetLayout()
        }
        .contentMargins(.horizontal, 20, for: .scrollContent)
        .scrollTargetBehavior(.viewAligned)
        .scrollIndicators(.hidden)
        .scrollClipDisabled()
    }

    private var chips: some View {
        ScrollView(.horizontal) {
            GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                ForEach(StoreFilter.allCases) { f in
                    let on = filter == f
                    Button {
                        Haptics.selectionTick()
                        withAnimation(.snappy) { filter = f }
                    } label: {
                        Label { Text(f.label) } icon: { ZoenIcon(f.glyph, size: 15) }
                            .font(.system(size: 14, weight: .semibold))
                            .labelStyle(.titleAndIcon)
                            .foregroundStyle(on ? Palette.background : Palette.textPrimary)
                            .padding(.horizontal, 14)
                            .frame(height: 36)
                            .background { if on { Capsule().fill(Palette.textPrimary) } }
                            .glassChip(on: on)
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
            } // GlassEffectContainer
        }
        .contentMargins(.horizontal, 20, for: .scrollContent)
        .scrollIndicators(.hidden)
        .scrollClipDisabled()
    }
}

private extension View {
    /// Unselected chips are Liquid Glass; the selected one is a solid ink pill.
    @ViewBuilder func glassChip(on: Bool) -> some View {
        if on { self } else { self.glassEffect(.regular.interactive(), in: .capsule) }
    }
}

/// Shared tab title row: big title on the left; optional leading actions, then bell + avatar.
struct TabHeader<Trailing: View>: View {
    @Environment(AppModel.self) private var model
    let title: String
    @ViewBuilder var trailing: () -> Trailing

    init(title: String, @ViewBuilder trailing: @escaping () -> Trailing = { EmptyView() }) {
        self.title = title
        self.trailing = trailing
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(title)
                .font(.system(size: 32, weight: .bold))
                .foregroundStyle(Palette.textPrimary)
                .accessibilityAddTraits(.isHeader)
            Spacer()
            trailing()
            NotificationsBell()
            if let me = model.me {
                Button { Haptics.tap(); model.select(.you) } label: { Avatar(persona: me, size: 40) }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Your context")
            }
        }
    }
}

/// "Store" title on the left; the notifications bell and your avatar on the right.
struct StoreHeader: View {
    var body: some View { TabHeader(title: AppTab.store.title) }
}

/// The bell in a glass circle (Home and Store headers): unread badge = what's waiting for
/// you (approvals first). Opens the notifications sheet, where approvals live.
struct NotificationsBell: View {
    @Environment(AppModel.self) private var model
    var body: some View {
        Button {
            Haptics.tap()
            model.openNotifications()
        } label: {
            ZoenIcon(.bell, size: 21)
                .foregroundStyle(Palette.textPrimary)
                .frame(width: 40, height: 40)
                .glassEffect(.regular.interactive(), in: .circle)
                .overlay(alignment: .topTrailing) {
                    if model.pendingCount > 0 {
                        Text("\(min(model.pendingCount, 99))")
                            .font(.system(size: 11, weight: .bold)).monospacedDigit()
                            .foregroundStyle(.white)
                            .padding(.horizontal, 5).frame(minWidth: 18, minHeight: 18)
                            .background(Palette.danger, in: .capsule)
                            .offset(x: 5, y: -4)
                            .contentTransition(.numericText())
                    }
                }
        }
        .buttonStyle(IconPressStyle())
        .accessibilityLabel(model.pendingCount > 0 ? String(localized: "Notifications, \(model.pendingCount) waiting for you") : String(localized: "Notifications"))
    }
}

// MARK: - Art pieces (pen and paper)

/// A mini-app's icon: a paper rounded square with its doodle.
struct InkAppIcon: View {
    let doodle: Doodle
    var accent: Color = InkPalette.mint
    var size: CGFloat = 56
    var live = true
    var body: some View {
        ZStack {
            PaperBackground(seed: doodle.seed, wash: accent)
            DoodleView(doodle: doodle, drawOn: live ? 1.1 : 0, freezeAt: live ? nil : 3).padding(size * 0.1)
        }
        .frame(width: size, height: size)
        .clipShape(.rect(cornerRadius: size * 0.26, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: size * 0.26, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.12), lineWidth: 0.5))
        .accessibilityHidden(true)
    }
}

/// An agent's avatar: round, like any contact, drawn in ink on paper.
struct InkAgentAvatar: View {
    let doodle: Doodle
    var accent: Color = InkPalette.mint
    var size: CGFloat = 56
    var body: some View {
        ZStack {
            PaperBackground(seed: doodle.seed &+ 3, wash: accent)
            DoodleView(doodle: doodle).padding(size * 0.12)
        }
        .frame(width: size, height: size)
        .clipShape(.circle)
        .overlay {
            InkDrawing(seed: doodle.seed &+ 41, drawOn: 0, fps: 8, breathe: 0, jitter: 0.6) { _ in
                [InkStroke.ellipse(0.5, 0.5, 0.475, 0.475, width: max(0.02, 1.6 / size), color: InkPalette.ink.opacity(0.75), n: 14)]
            }
        }
        .accessibilityHidden(true)
    }
}

struct ListingIcon: View {
    let listing: StoreListing
    var size: CGFloat = 56
    var body: some View {
        if listing.isAgent { InkAgentAvatar(doodle: listing.doodle, accent: listing.accent, size: size) }
        else { InkAppIcon(doodle: listing.doodle, accent: listing.accent, size: size) }
    }
}

/// Collection cover: warm paper, a big doodle with a colour wash, and the text on a paper
/// scrim under a hand-drawn rule.
struct CollectionCard: View {
    let collection: StoreCollection
    let width: CGFloat
    private var height: CGFloat { (width * 1.1).rounded() }

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            PaperBackground(seed: collection.doodle.seed &* 7, wash: collection.wash)
            DoodleView(doodle: collection.doodle)
                .frame(width: width * 0.62, height: width * 0.62)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .padding(.top, height * 0.06)
            VStack(alignment: .leading, spacing: 6) {
                InkRule(seed: collection.doodle.seed).frame(height: 8).padding(.horizontal, -4)
                Text(collection.title)
                    .font(.system(size: 22, weight: .bold))
                    .foregroundStyle(InkPalette.ink)
                    .lineLimit(1).minimumScaleFactor(0.8)
                Text(collection.subtitle)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(InkPalette.ink.opacity(0.68))
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: -8) {
                    ForEach(collection.items.compactMap(StoreSeed.listing), id: \.id) { l in
                        Group {
                            if l.isAgent { InkAgentAvatar(doodle: l.doodle, accent: l.accent, size: 32) }
                            else { InkAppIcon(doodle: l.doodle, accent: l.accent, size: 32, live: false).clipShape(.circle) }
                        }
                        .overlay(Circle().strokeBorder(Paper.sheet, lineWidth: 2))
                    }
                }
                .padding(.top, 4)
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
            .padding(.bottom, 18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Paper.sheet.opacity(0.94))
        }
        .frame(width: width, height: height)
        .clipShape(.rect(cornerRadius: 28, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 28, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.1), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.08), radius: 12, y: 6)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(collection.title). \(collection.subtitle)")
    }
}

/// Featured: a paper card with the doodled object centred (agents: round, like a contact),
/// the title in the listing's colour, two lines of copy and the creator chip.
struct FeaturedCard: View {
    @Environment(AppModel.self) private var model
    let listing: StoreListing
    let width: CGFloat

    var body: some View {
        VStack(spacing: 8) {
            Group {
                if listing.isAgent {
                    InkAgentAvatar(doodle: listing.doodle, accent: listing.accent, size: width * 0.5)
                } else {
                    DoodleView(doodle: listing.doodle).frame(width: width * 0.56, height: width * 0.56)
                }
            }
            .frame(height: width * 0.58)
            .padding(.top, 14)
            VStack(spacing: 0) {
                Text(listing.name)
                if let sub = listing.subtitle { Text(sub).foregroundStyle(InkPalette.ink) }
            }
            .font(.system(size: 19, weight: .semibold))
            .foregroundStyle(listing.accent)
            .multilineTextAlignment(.center)
            .lineLimit(2)
            Text(listing.blurb)
                .font(.system(size: 13))
                .foregroundStyle(InkPalette.ink.opacity(0.62))
                .multilineTextAlignment(.center)
                .lineLimit(2)
            Spacer(minLength: 2)
            if listing.isAgent {
                HStack(spacing: 8) {
                    CreatorChip(handle: listing.creator)
                    MessagePill(listing: listing, compact: true)
                }
            } else {
                CreatorChip(handle: listing.creator)
            }
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
        .frame(width: width, height: (width * 1.24).rounded())
        .background(PaperBackground(seed: listing.doodle.seed &* 13))
        .clipShape(.rect(cornerRadius: 28, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 28, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.08), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.07), radius: 14, y: 6)
        .accessibilityElement(children: .combine)
    }
}

struct CreatorChip: View {
    let handle: String
    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(Color(hex: Self.color(handle))).frame(width: 16, height: 16)
                .overlay(Text(String(handle.prefix(1)).uppercased()).font(.system(size: 9, weight: .bold)).foregroundStyle(.white))
            Text(verbatim: "@\(handle)").font(.system(size: 12, weight: .medium)).foregroundStyle(InkPalette.ink.opacity(0.75))
        }
        .padding(.leading, 4).padding(.trailing, 9).frame(height: 24)
        .background(InkPalette.ink.opacity(0.06), in: .capsule)
    }
    static func color(_ h: String) -> String {
        let palette = ["#E2725B", "#4F86C6", "#3D8A3A", "#B07A12", "#8E63C7", "#D0453A", "#17808A"]
        return palette[abs(h.unicodeScalars.reduce(0) { $0 &+ Int($1.value) }) % palette.count]
    }
}

/// "Message" on an agent: opens a normal chat with it when it's one of your contacts.
struct MessagePill: View {
    @Environment(AppModel.self) private var model
    let listing: StoreListing
    var compact = false
    var body: some View {
        Button {
            Haptics.tap()
            model.messageAgent(listing)
        } label: {
            Text(AppLocale.pick("Mensagem", "Message"))
                .font(.system(size: compact ? 12 : 14, weight: .bold))
                .foregroundStyle(.white)
                .padding(.horizontal, compact ? 10 : 16)
                .frame(height: compact ? 24 : 32)
                .background(Palette.action, in: .capsule)
        }
        .buttonStyle(.plain)
    }
}

struct GetPill: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let listing: StoreListing
    @State private var stamp = false
    @Namespace private var pillNS
    var body: some View {
        let has = model.storeInstalled.contains(listing.id)
        Button {
            if has {
                Haptics.tap()
                model.installListing(listing)
            } else {
                let first = WowGate.once("stamp-\(listing.id)")
                model.installListing(listing)
                if first && !reduceMotion {
                    withAnimation(.spring(duration: 0.45, bounce: 0.32)) { stamp = true }
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.1) {
                        withAnimation(.easeOut(duration: 0.25)) { stamp = false }
                    }
                }
            }
        } label: {
            Text(has ? AppLocale.pick("Abrir", "Open") : AppLocale.pick("Obter", "Get"))
                .font(.system(size: 14, weight: .bold))
                .foregroundStyle(Palette.action)
                .padding(.horizontal, 16)
                .frame(height: 32)
                .glassEffect(.regular.tint(Palette.action.opacity(0.14)).interactive(), in: .capsule)
                .glassEffectID("get-\(listing.id)", in: pillNS)
        }
        .buttonStyle(.plain)
        .overlay {
            if stamp {
                InkStampOverlay(portuguese: AppLocale.isPortuguese)
                    .transition(.scale(scale: 1.4).combined(with: .opacity))
                    .offset(y: -8)
            }
        }
    }
}


struct InviteCard: View {
    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                Text(AppLocale.pick("Você tem 1 convite!", "You've got 1 invite!"))
                    .font(.system(size: 20, weight: .bold)).foregroundStyle(Palette.textPrimary)
                Text(AppLocale.pick("Apps bons começam com gente boa. Convide amigos de bom gosto pra ajudar a melhorar o Zoen.", "Great apps start with great people. Invite friends with taste who can help improve Zoen."))
                    .font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            ZStack {
                InkAgentAvatar(doodle: .moss, accent: InkPalette.mint, size: 38).offset(x: -18, y: -14)
                InkAgentAvatar(doodle: .heart, accent: InkPalette.tomato, size: 34).offset(x: 20, y: -16)
                InkAgentAvatar(doodle: .vinyl, accent: InkPalette.lilac, size: 44).offset(x: 2, y: 12)
                ZoenIcon(.plus, size: 13).foregroundStyle(.white)
                    .frame(width: 22, height: 22).background(Palette.textPrimary, in: .circle)
                    .overlay(Circle().strokeBorder(Palette.surface, lineWidth: 2))
                    .offset(x: 22, y: 26)
            }
            .frame(width: 86, height: 86)
        }
        .padding(18)
        .glassEffect(.regular, in: .rect(cornerRadius: 28, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

struct RankedRow: View {
    let rank: Int
    let listing: StoreListing
    var body: some View {
        HStack(spacing: 14) {
            ListingIcon(listing: listing, size: 56)
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: "\(rank). \(listing.name)").font(.system(size: 16, weight: .bold)).foregroundStyle(Palette.textPrimary).lineLimit(1)
                Text(listing.blurb).font(.system(size: 13)).foregroundStyle(Palette.textSecondary).lineLimit(1)
                Text(verbatim: "@\(listing.creator)").font(.system(size: 12)).foregroundStyle(Palette.textTertiary)
            }
            Spacer(minLength: 6)
            if listing.isAgent { MessagePill(listing: listing) } else { GetPill(listing: listing) }
        }
        .padding(.vertical, 6)
        .contentShape(.rect)
        .accessibilityElement(children: .combine)
    }
}

struct FollowCard: View {
    let listing: StoreListing
    let by: String
    let width: CGFloat
    var body: some View {
        VStack(spacing: 6) {
            Group {
                if listing.isAgent { InkAgentAvatar(doodle: listing.doodle, accent: listing.accent, size: width * 0.5) }
                else { DoodleView(doodle: listing.doodle).frame(width: width * 0.56, height: width * 0.56) }
            }
            .padding(.top, 12)
            Text(listing.name).font(.system(size: 15, weight: .semibold)).foregroundStyle(listing.accent).lineLimit(1)
            Text(listing.blurb).font(.system(size: 12)).foregroundStyle(InkPalette.ink.opacity(0.6))
                .multilineTextAlignment(.center).lineLimit(2)
            Spacer(minLength: 0)
            Text(verbatim: "@\(by)").font(.system(size: 11, weight: .medium)).foregroundStyle(InkPalette.ink.opacity(0.5))
        }
        .padding(.horizontal, 10).padding(.bottom, 12)
        .frame(width: width, height: (width * 1.25).rounded())
        .background(PaperBackground(seed: listing.doodle.seed &* 17))
        .clipShape(.rect(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.08), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.06), radius: 10, y: 4)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Detail (stub)

struct StoreDetailView: View {
    @Environment(AppModel.self) private var model
    let listingId: String

    var body: some View {
        if let l = StoreSeed.listing(listingId) {
            ScrollView {
                VStack(spacing: 14) {
                    ZStack {
                        PaperBackground(seed: l.doodle.seed &* 19, wash: l.accent)
                        if l.isAgent { InkAgentAvatar(doodle: l.doodle, accent: l.accent, size: 150) }
                        else { DoodleView(doodle: l.doodle).frame(width: 170, height: 170) }
                    }
                    .frame(height: 230)
                    .clipShape(.rect(cornerRadius: 30, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 30, style: .continuous).strokeBorder(InkPalette.ink.opacity(0.08), lineWidth: 0.5))

                    VStack(spacing: 4) {
                        Text(l.name).font(.system(size: 26, weight: .bold)).foregroundStyle(l.accent).multilineTextAlignment(.center)
                        Text(verbatim: "@\(l.creator)").font(.subheadline.weight(.medium)).foregroundStyle(Palette.textSecondary)
                    }

                    Group {
                        if l.isAgent {
                            Button { Haptics.tap(); model.messageAgent(l) } label: {
                                Label { Text(AppLocale.pick("Mensagem", "Message")) } icon: { ZoenIcon(.chats, size: 19) }.frame(maxWidth: .infinity)
                            }
                        } else {
                            let has = model.storeInstalled.contains(l.id)
                            Button { Haptics.tap(); model.installListing(l) } label: {
                                Label { Text(has ? AppLocale.pick("Instalado", "Installed") : AppLocale.pick("Instalar", "Install")) }
                                      icon: { ZoenIcon(has ? .check : .download, size: 19) }.frame(maxWidth: .infinity)
                            }
                        }
                    }
                    .font(.headline)
                    .buttonStyle(.glassProminent)
                    .controlSize(.large)
                    .tint(Palette.action)

                    HStack(spacing: 0) {
                        stat(l.rating, AppLocale.pick("nota", "rating"))
                        Divider().frame(height: 28)
                        stat(l.uses, AppLocale.pick("chats", "chats"))
                        Divider().frame(height: 28)
                        stat(l.isAgent ? AppLocale.pick("Agente", "Agent") : "Mini-app", AppLocale.pick("tipo", "kind"))
                    }
                    .padding(.vertical, 10)
                    .glassEffect(.regular, in: .rect(cornerRadius: 18, style: .continuous))

                    section(AppLocale.pick("Sobre", "About")) {
                        Text(l.about).font(.body).foregroundStyle(Palette.textPrimary).fixedSize(horizontal: false, vertical: true)
                    }
                    section(AppLocale.pick("Criador", "Creator")) {
                        HStack(spacing: 10) {
                            Circle().fill(Color(hex: CreatorChip.color(l.creator))).frame(width: 36, height: 36)
                                .overlay(Text(String(l.creator.prefix(1)).uppercased()).font(.headline).foregroundStyle(.white))
                            VStack(alignment: .leading, spacing: 1) {
                                Text(verbatim: "@\(l.creator)").font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                                Text(AppLocale.pick("3 apps na Loja", "3 apps in the Store")).font(.caption).foregroundStyle(Palette.textSecondary)
                            }
                            Spacer()
                            Button(AppLocale.pick("Seguir", "Follow")) { model.show(.init(kind: .info, text: AppLocale.pick("Dados de exemplo: seguir chega com a Loja de verdade.", "Seed data: following comes with the real Store."))) }
                                .buttonStyle(.glass)
                        }
                    }
                    section(AppLocale.pick("Comunidade", "Community")) {
                        VStack(alignment: .leading, spacing: 10) {
                            Label { Text(l.community) } icon: { ZoenIcon(.spaces, size: 17) }.font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                            review("marina", AppLocale.pick("A gente usa toda semana no grupo. Simples e bonito.", "We use it every week in the group. Simple and lovely."))
                            review("lucas", AppLocale.pick("Pediu permissão certinho antes de mexer no calendário.", "Asked properly before touching the calendar."))
                        }
                    }
                    Label { Text(AppLocale.pick("Dados de exemplo: esta página é um esboço.", "Seed data: this page is a stub.")) } icon: { ZoenIcon(.info, size: 14) }
                        .font(.caption).foregroundStyle(Palette.textTertiary)
                        .padding(.top, 4)
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 24)
            }
            .background(Palette.background.ignoresSafeArea())
            .navigationTitle(l.name)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
        } else {
            ContentUnavailableView("Not found", systemImage: "questionmark")
        }
    }

    private func stat(_ v: String, _ label: String) -> some View {
        VStack(spacing: 2) {
            Text(v).font(.headline).foregroundStyle(Palette.textPrimary)
            Text(label).font(.caption).foregroundStyle(Palette.textSecondary)
        }
        .frame(maxWidth: .infinity)
    }

    private func section<C: View>(_ title: String, @ViewBuilder _ c: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.headline).foregroundStyle(Palette.textPrimary).accessibilityAddTraits(.isHeader)
            c()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .glassEffect(.regular, in: .rect(cornerRadius: 20, style: .continuous))
    }

    private func review(_ who: String, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Circle().fill(Color(hex: CreatorChip.color(who))).frame(width: 24, height: 24)
                .overlay(Text(String(who.prefix(1)).uppercased()).font(.caption2.weight(.bold)).foregroundStyle(.white))
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: "@\(who)").font(.caption.weight(.semibold)).foregroundStyle(Palette.textSecondary)
                Text(text).font(.subheadline).foregroundStyle(Palette.textPrimary)
            }
        }
    }
}

// MARK: - Model hooks (seed behaviour)

extension AppModel {
    /// Agents are contacts: Message opens the 1:1 chat when the agent is on this device.
    func messageAgent(_ l: StoreListing) {
        if let h = l.handle, let id = spaces.first(where: { $0.counterpart?.handle == h })?.id {
            go(.space(id))
        } else {
            show(.init(kind: .info, text: AppLocale.pick("Agente de exemplo: na Loja de verdade, Mensagem abre um chat 1:1 com ele.", "Seed agent: in the real Store, Message opens a 1:1 chat with it.")))
        }
    }

    /// Stub install: remembers it locally; nothing is downloaded.
    func installListing(_ l: StoreListing) {
        if storeInstalled.contains(l.id) {
            show(.init(kind: .info, text: AppLocale.pick("Abra pelo chat onde você usa.", "Open it from the chat where you use it.")))
        } else {
            storeInstalled.insert(l.id)
            Haptics.commit()
            show(.init(kind: .info, text: AppLocale.pick("Instalado (dados de exemplo, nada foi baixado).", "Installed (seed data, nothing was downloaded).")))
        }
    }
}

/// A hand-drawn rule across the full width (the collection scrim's top edge), boiling.
struct InkRule: View {
    var seed: UInt64 = 1
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 8, paused: reduceMotion)) { tl in
            let frame = reduceMotion ? 0 : Int(tl.date.timeIntervalSinceReferenceDate * 8) % 4
            Canvas { ctx, size in
                var rng = InkRNG(seed &+ UInt64(frame) &* 31)
                let n = 9
                let pts = (0...n).map { i in CGPoint(x: size.width * CGFloat(i) / CGFloat(n), y: size.height / 2 + rng.signed() * 1.4) }
                ctx.fill(Ink.ribbon(Ink.catmull(pts, closed: false), width: 1.5, tapered: true, rng: &rng), with: .color(InkPalette.ink.opacity(0.45)))
            }
        }
        .accessibilityHidden(true)
    }
}
