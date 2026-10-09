import SwiftUI
import ImageIO
import UniformTypeIdentifiers
import RodaCore

/// An agent using its browser in a chat (ADR 0028 §7), as the owner sees it on the phone.
///
/// The screen arrives sealed to this device's live-view key and is opened here (`LiveViewSession`),
/// so only the owner sees it. When the site asks for a password or a code the agent stops
/// and asks; the owner takes over, types (sealed the same way), and "Pronto" hands it back.
/// Until the relay carries real sessions, `LiveViewDemoVm` plays the browser's half with the
/// same crypto and a drawn page (`DemoSitePage`).
@MainActor @Observable
final class AgentBrowser {
    enum Phase: Equatable {
        /// The agent is browsing; the owner can watch.
        case browsing
        /// The site asks for something only the owner may type: the takeover card.
        case needsYou
        /// "Agora não": the agent waits where it stopped.
        case waiting
        /// The owner has the browser; the agent is stopped.
        case driving
        /// Handed back and finished.
        case finished
    }

    struct Session: Equatable {
        let spaceId: String
        let agent: Persona
        let site: String
        let task: String
    }

    private(set) var session: Session?
    private(set) var phase: Phase = .browsing
    private(set) var frame: CGImage?
    /// Characters the page holds in its password field (the VM counts them; nobody else sees them).
    private(set) var typed: UInt32 = 0
    var screenOpen = false

    private var step = 0
    private var vm: LiveViewDemoVm?
    private var live: LiveViewSession?
    private var loop: Task<Void, Never>?

    /// Starts the showcase session: an agent booking the inn from the trip plan.
    func startDemo(spaceId: String, agent: Persona) {
        let key = Self.deviceKey()
        guard let vm = try? LiveViewDemoVm.start(devicePub: key.publicKey()),
              let live = try? key.accept(vmPub: vm.vmPub(), session: vm.session()) else { return }
        self.vm = vm
        self.live = live
        session = Session(spaceId: spaceId, agent: agent, site: "casaazulparaty.com.br",
                          task: String(localized: "Booking Pousada Casa Azul"))
        phase = .browsing
        step = 0
        typed = 0
        push()
        loop?.cancel()
        loop = Task { @MainActor [weak self] in
            for next in 1...3 {
                try? await Task.sleep(for: .seconds(1.6))
                guard let self, !Task.isCancelled else { return }
                self.step = next
                self.push()
            }
            guard let self, !Task.isCancelled else { return }
            Haptics.warning()
            withAnimation(.spring(response: 0.45, dampingFraction: 0.75)) { self.phase = .needsYou }
        }
    }

    func takeOver() {
        guard phase == .needsYou || phase == .waiting else { return }
        Haptics.open()
        withAnimation(.spring(response: 0.4, dampingFraction: 0.85)) { phase = .driving }
        screenOpen = true
    }

    func notNow() {
        guard phase == .needsYou else { return }
        Haptics.dismiss()
        withAnimation(.snappy) { phase = .waiting }
    }

    /// The password field changed on the phone: what was added goes up as text, what was
    /// removed as Backspace. Only the count comes back.
    func typeChanged(from old: String, to new: String) {
        guard phase == .driving, let live else { return }
        let common = old.commonPrefix(with: new)
        for _ in 0..<(old.count - common.count) { deliver(live.sealKey(key: "Backspace")) }
        let added = new.dropFirst(common.count)
        if !added.isEmpty { deliver(live.sealText(text: String(added))) }
        push()
    }

    /// A tap on the page while you drive.
    func click(x: Double, y: Double) {
        guard phase == .driving, let live else { return }
        Haptics.selectionTick()
        deliver(live.sealClick(x: x, y: y))
    }

    /// "Pronto": the only thing that hands the browser back.
    func done() {
        guard phase == .driving, let live else { return }
        deliver(live.sealDone())
    }

    func close() { screenOpen = false }

    private func deliver(_ sealed: Data) {
        guard let vm, let input = try? vm.openInput(sealed: sealed) else { return }
        switch input {
        case .text(let chars): typed += chars
        case .key(let key): if key == "Backspace", typed > 0 { typed -= 1 }
        case .click: break
        case .done:
            Haptics.commit()
            step = 4
            withAnimation(.spring(response: 0.45, dampingFraction: 0.8)) { phase = .finished }
            push()
        }
    }

    /// One frame: drawn by the "VM", sealed, opened with this device's key, shown.
    private func push() {
        guard let vm, let live else { return }
        let page = DemoSitePage(step: step, dots: Int(typed)).frame(width: 360, height: 480)
        let renderer = ImageRenderer(content: page)
        renderer.scale = 2
        guard let cg = renderer.cgImage, let jpeg = Self.jpeg(cg) else { return }
        let sealed = vm.sealFrame(jpeg: jpeg)
        guard let opened = try? live.openFrame(sealed: sealed), let image = Self.decode(opened) else { return }
        frame = image
    }

    private static func jpeg(_ image: CGImage) -> Data? {
        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: 0.72] as CFDictionary)
        return CGImageDestinationFinalize(dest) ? out as Data : nil
    }

    private static func decode(_ data: Data) -> CGImage? {
        guard let src = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(src, 0, nil)
    }

    /// This device's live-view key: made once, kept in the Keychain, never leaves the phone.
    private static func deviceKey() -> LiveViewKey {
        let vault = KeychainVault()
        if let secret = vault.load(key: "liveview-device"), let key = try? LiveViewKey.restore(secret: secret) { return key }
        let key = LiveViewKey.generate()
        _ = vault.save(key: "liveview-device", value: key.secret())
        return key
    }
}

// MARK: - The card in the chat

/// The agent's browser in the chat: a live thumbnail while it works, the takeover card when
/// the site needs you, and what happened once it's done.
struct AgentBrowserCard: View {
    @Environment(AppModel.self) private var model
    @State private var nudge = false

    var body: some View {
        let browser = model.browser
        if let s = browser.session {
            VStack(alignment: .leading, spacing: 10) {
                header(s, browser.phase)
                thumbnail(browser, s)
                    .onTapGesture { Haptics.tap(); browser.screenOpen = true }
                    .accessibilityElement(children: .ignore)
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel(Text("Watch the screen"))
                    .accessibilityIdentifier("browser-live")
                footer(s, browser)
            }
            .padding(14)
            .background(RoundedRectangle(cornerRadius: 22, style: .continuous).fill(Palette.surface))
            .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous)
                .strokeBorder(browser.phase == .needsYou ? InkPalette.butter : Palette.hairline,
                              lineWidth: browser.phase == .needsYou ? 2 : 1))
            .shadow(color: .black.opacity(0.06), radius: 10, y: 4)
            .scaleEffect(nudge ? 1.015 : 1)
            .padding(.top, 10)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("browser-card")
            .onChange(of: browser.phase) { _, p in
                guard p == .needsYou else { return }
                withAnimation(.spring(response: 0.25, dampingFraction: 0.5)) { nudge = true }
                Task { @MainActor in
                    try? await Task.sleep(for: .milliseconds(220))
                    withAnimation(.spring(response: 0.35, dampingFraction: 0.7)) { nudge = false }
                }
            }
        }
    }

    @ViewBuilder private func header(_ s: AgentBrowser.Session, _ phase: AgentBrowser.Phase) -> some View {
        HStack(spacing: 8) {
            ContactAvatar(persona: s.agent, size: 24)
            Group {
                switch phase {
                case .browsing: Text("\(s.agent.name) is using the browser")
                case .needsYou: Text("\(s.agent.name) needs you")
                case .waiting: Text("\(s.agent.name) is waiting for you")
                case .driving: Text("You have the browser")
                case .finished: Text("Booking confirmed")
                }
            }
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Palette.textPrimary)
            .contentTransition(.opacity)
            .accessibilityIdentifier("browser-title")
            Spacer(minLength: 6)
            if phase == .finished {
                Image(systemName: "checkmark.seal.fill").foregroundStyle(Palette.action)
                    .transition(.scale.combined(with: .opacity))
            } else {
                LiveDot(paused: phase == .waiting)
            }
        }
    }

    private func thumbnail(_ browser: AgentBrowser, _ s: AgentBrowser.Session) -> some View {
        ZStack(alignment: .bottomLeading) {
            Rectangle().fill(Palette.surfaceMuted)
            if let f = browser.frame {
                Image(decorative: f, scale: 2)
                    .resizable()
                    .scaledToFill()
                    .frame(maxHeight: .infinity, alignment: .top)
                    .transition(.opacity)
            }
            SiteChip(site: s.site).padding(8)
        }
        .frame(height: 150)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .animation(.easeOut(duration: 0.2), value: browser.frame == nil)
    }

    @ViewBuilder private func footer(_ s: AgentBrowser.Session, _ browser: AgentBrowser) -> some View {
        switch browser.phase {
        case .browsing:
            Text("\(s.task). Only you can see this screen.")
                .font(.caption).foregroundStyle(Palette.textSecondary)
        case .needsYou, .waiting:
            VStack(alignment: .leading, spacing: 10) {
                Text("The site is asking for your password. \(s.agent.name) doesn't type passwords: you sign in, then it carries on.")
                    .font(.footnote).foregroundStyle(Palette.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 10) {
                    if browser.phase == .needsYou {
                        Button { browser.notNow() } label: {
                            Text("Not now").font(.subheadline.weight(.semibold)).frame(maxWidth: .infinity).padding(.vertical, 11)
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.textPrimary)
                        .background(Capsule().fill(Palette.surfaceMuted))
                        .accessibilityIdentifier("browser-not-now")
                    }
                    Button { browser.takeOver() } label: {
                        Label(String(localized: "Take over"), systemImage: "hand.point.up.left.fill")
                            .font(.subheadline.weight(.semibold)).frame(maxWidth: .infinity).padding(.vertical, 11)
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.white)
                    .background(Capsule().fill(Palette.action))
                    .accessibilityIdentifier("browser-takeover")
                }
            }
            .transition(.opacity.combined(with: .move(edge: .bottom)))
        case .driving:
            Text("\(s.agent.name) is stopped while you use it.")
                .font(.caption).foregroundStyle(Palette.textSecondary)
        case .finished:
            Text("You signed in and \(s.agent.name) finished: 2 nights, Oct 17 to 19.")
                .font(.caption).foregroundStyle(Palette.textSecondary)
                .accessibilityIdentifier("browser-finished")
        }
    }
}

/// A small red dot that breathes while the screen is live.
struct LiveDot: View {
    var paused = false
    @State private var on = false
    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(paused ? Palette.textTertiary : Color.red)
                .frame(width: 7, height: 7)
                .opacity(paused ? 1 : (on ? 1 : 0.35))
                .animation(paused ? nil : .easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: on)
            Text(paused ? String(localized: "Paused") : String(localized: "Live"))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Palette.textSecondary)
        }
        .onAppear { on = true }
        .accessibilityElement(children: .combine)
    }
}

private struct SiteChip: View {
    let site: String
    var body: some View {
        Label(site, systemImage: "lock.fill")
            .font(.caption2.weight(.medium))
            .lineLimit(1)
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background(.ultraThinMaterial, in: Capsule())
    }
}

// MARK: - Full screen

/// Watching, and taking over: the page fills the screen; while you drive, the agent is stopped
/// and a password typed here goes only to the site.
struct LiveViewScreen: View {
    @Environment(AppModel.self) private var model
    @State private var password = ""
    @FocusState private var focused: Bool

    var body: some View {
        let browser = model.browser
        VStack(spacing: 14) {
            HStack {
                Button { Haptics.dismiss(); browser.close() } label: {
                    Image(systemName: "chevron.down")
                        .font(.body.weight(.semibold))
                        .frame(width: 40, height: 40)
                        .background(Circle().fill(Palette.surfaceMuted))
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.textPrimary)
                .accessibilityLabel(Text("Back"))
                .accessibilityIdentifier("browser-close")
                Spacer()
                if let s = browser.session { SiteChip(site: s.site) }
                Spacer()
                LiveDot(paused: browser.phase == .waiting || browser.phase == .finished).frame(width: 40 + 20)
            }
            .padding(.horizontal, 16)

            GeometryReader { g in
                ZStack {
                    if let f = browser.frame {
                        Image(decorative: f, scale: 2)
                            .resizable()
                            .scaledToFit()
                            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                            .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous)
                                .strokeBorder(browser.phase == .driving ? Palette.action : Palette.hairline,
                                              lineWidth: browser.phase == .driving ? 3 : 1))
                            .shadow(color: .black.opacity(0.12), radius: 16, y: 6)
                            .onTapGesture { p in
                                let fit = min(g.size.width / 360, g.size.height / 480)
                                browser.click(x: Double(p.x / fit), y: Double(p.y / fit))
                            }
                    }
                }
                .frame(width: g.size.width, height: g.size.height)
            }
            .padding(.horizontal, 16)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text("The agent's screen"))
            .accessibilityIdentifier("browser-screen")

            panel(browser)
                .padding(.horizontal, 16)
                .padding(.bottom, 12)
        }
        .padding(.top, 12)
        .background(Palette.background.ignoresSafeArea())
        .onChange(of: password) { old, new in browser.typeChanged(from: old, to: new) }
        .onChange(of: browser.phase) { _, p in focused = p == .driving }
        .onAppear { if browser.phase == .driving { focused = true } }
    }

    @ViewBuilder private func panel(_ browser: AgentBrowser) -> some View {
        let name = browser.session?.agent.name ?? ""
        switch browser.phase {
        case .driving:
            VStack(spacing: 10) {
                Label(String(localized: "\(name) is stopped while you use it, and can't see what you type."),
                      systemImage: "pause.circle.fill")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Palette.textSecondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("browser-stopped")
                SecureField(String(localized: "Your password for the site"), text: $password)
                    #if os(iOS)
                    .textContentType(.password)
                    #endif
                    .focused($focused)
                    .padding(.horizontal, 14).padding(.vertical, 12)
                    .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Palette.surface))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Palette.hairline))
                    .accessibilityIdentifier("browser-password")
                Button {
                    focused = false
                    browser.done()
                } label: {
                    Text("Done, carry on").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 13)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.white)
                .background(Capsule().fill(browser.typed == 0 ? Palette.textTertiary : Palette.action))
                .disabled(browser.typed == 0)
                .accessibilityIdentifier("browser-done")
            }
            .transition(.move(edge: .bottom).combined(with: .opacity))
        case .needsYou, .waiting:
            VStack(spacing: 10) {
                Text("\(name) needs your password to go on.")
                    .font(.subheadline).foregroundStyle(Palette.textSecondary)
                Button { browser.takeOver() } label: {
                    Label(String(localized: "Take over"), systemImage: "hand.point.up.left.fill")
                        .font(.headline).frame(maxWidth: .infinity).padding(.vertical, 13)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.white)
                .background(Capsule().fill(Palette.action))
                .accessibilityIdentifier("browser-takeover-screen")
            }
        case .browsing:
            if let s = browser.session {
                Text("\(s.task). Only you can see this screen.")
                    .font(.subheadline).foregroundStyle(Palette.textSecondary)
                    .frame(maxWidth: .infinity)
            }
        case .finished:
            VStack(spacing: 10) {
                Text("Done. \(name) confirmed the booking.")
                    .font(.subheadline.weight(.medium)).foregroundStyle(Palette.textPrimary)
                Button { Haptics.dismiss(); browser.close() } label: {
                    Text("Back to the chat").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 13)
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.textPrimary)
                .background(Capsule().fill(Palette.surfaceMuted))
                .accessibilityIdentifier("browser-back-to-chat")
            }
        }
    }
}

// MARK: - The drawn page (showcase only)

/// What the showcase "browser" shows: an inn's booking site, step by step. A website, so
/// it keeps its own light colors in dark mode.
struct DemoSitePage: View {
    let step: Int
    let dots: Int
    private let blue = Color(red: 0.13, green: 0.36, blue: 0.67)

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Casa Azul").font(.system(size: 20, weight: .bold, design: .serif)).foregroundStyle(blue)
                Spacer()
                Text("Paraty · RJ").font(.system(size: 12)).foregroundStyle(.gray)
            }
            .padding(.horizontal, 18).frame(height: 52)
            LinearGradient(colors: [Color(red: 0.55, green: 0.80, blue: 0.95), Color(red: 0.18, green: 0.55, blue: 0.75)],
                           startPoint: .top, endPoint: .bottom)
                .overlay(alignment: .bottomLeading) {
                    Text(step >= 3 ? "Confirmar reserva" : "Pousada no centro histórico")
                        .font(.system(size: 18, weight: .semibold)).foregroundStyle(.white).padding(14)
                }
                .frame(height: step >= 3 ? 70 : 130)
            VStack(alignment: .leading, spacing: 12) {
                row("Datas", step >= 1 ? "sex 17 – dom 19 out" : "Escolha as datas", done: step >= 1)
                row("Hóspedes", "2 adultos", done: step >= 1)
                row("Quarto", step >= 2 ? "Jardim · café incluso · R$ 640" : "—", done: step >= 2)
                if step == 3 {
                    Text("Entre para confirmar").font(.system(size: 15, weight: .semibold)).padding(.top, 6)
                    field("E-mail", "enzo@tironi.xyz")
                    field("Senha", dots == 0 ? "" : String(repeating: "•", count: min(dots, 18)), focused: true)
                }
                if step >= 4 {
                    HStack(spacing: 8) {
                        Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
                        Text("Reserva confirmada · nº 48213").font(.system(size: 15, weight: .semibold))
                    }
                    .padding(.top, 8)
                }
                Spacer(minLength: 0)
                Text(step >= 3 && step < 4 ? "Entrar" : "Continuar")
                    .font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
                    .frame(maxWidth: .infinity).padding(.vertical, 12)
                    .background(RoundedRectangle(cornerRadius: 10).fill(blue))
                    .opacity(step >= 4 ? 0 : 1)
            }
            .padding(18)
        }
        .foregroundStyle(Color(white: 0.12))
        .background(Color.white)
        .environment(\.colorScheme, .light)
    }

    private func row(_ k: String, _ v: String, done: Bool) -> some View {
        HStack {
            Text(k).font(.system(size: 13)).foregroundStyle(.gray)
            Spacer()
            Text(v).font(.system(size: 14, weight: done ? .semibold : .regular))
        }
        .padding(.vertical, 8)
        .overlay(alignment: .bottom) { Rectangle().fill(Color(white: 0.9)).frame(height: 1) }
    }

    private func field(_ label: String, _ value: String, focused: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.system(size: 12)).foregroundStyle(.gray)
            Text(value.isEmpty ? " " : value).font(.system(size: 15))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(RoundedRectangle(cornerRadius: 8).strokeBorder(focused ? blue : Color(white: 0.8), lineWidth: focused ? 2 : 1))
        }
    }
}
