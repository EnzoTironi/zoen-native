import SwiftUI
import RodaCore
#if os(iOS)
import UIKit
#endif

/// Every Ajustes screen. Pushed inside the Ajustes stack (`SettingsRoot`), so Back pops one
/// and the X closes Ajustes from anywhere.
enum SettingsScreen: String, Hashable, CaseIterable, Identifiable {
    case profile, preferences, theme, security, devices, twoStep, data, backup
    case accessibility, legal, feedback, about, tasks
    case disappearing, plans, discover, moderation, invites, wallet
    var id: String { rawValue }

    var title: LocalizedStringKey {
        switch self {
        case .profile: "Profile"
        case .preferences: "Preferences"
        case .theme: "Theme"
        case .security: "Security"
        case .devices: "Devices"
        case .twoStep: "Two-step verification"
        case .data: "Your data"
        case .backup: "Backup and recovery"
        case .accessibility: "Accessibility"
        case .legal: "Terms and privacy"
        case .feedback: "Feedback"
        case .about: "About"
        case .tasks: "Tasks"
        case .disappearing: "Disappearing messages"
        case .plans: "Credits and plans"
        case .discover: "Discover"
        case .moderation: "Moderation"
        case .invites: "Invites"
        case .wallet: "Wallet"
        }
    }

    var symbol: String {
        switch self {
        case .profile: "person"
        case .preferences: "slider.horizontal.3"
        case .theme: "circle.lefthalf.filled"
        case .security: "lock"
        case .devices: "laptopcomputer.and.iphone"
        case .twoStep: "checkmark.shield"
        case .data: "externaldrive"
        case .backup: "icloud"
        case .accessibility: "accessibility"
        case .legal: "doc.text"
        case .feedback: "bubble.left"
        case .about: "info.circle"
        case .tasks: "calendar.badge.clock"
        case .disappearing: "timer"
        case .plans: "sparkles"
        case .discover: "safari"
        case .moderation: "flag"
        case .invites: "gift"
        case .wallet: "creditcard"
        }
    }

    /// Extra words the Ajustes search matches (pt and en), so "senha" finds Segurança.
    var keywords: String {
        switch self {
        case .profile: "nome name foto photo arroba handle perfil"
        case .preferences: "preferências idioma language"
        case .theme: "tema escuro claro dark light aparência appearance"
        case .security: "segurança senha password código code"
        case .devices: "aparelhos celular computador iphone mac desconectar"
        case .twoStep: "duas etapas 2fa autenticador sms"
        case .data: "dados exportar apagar espaço storage"
        case .backup: "cópia backup restaurar recuperação senha"
        case .accessibility: "acessibilidade movimento texto contraste transparência"
        case .legal: "termos privacidade legal"
        case .feedback: "feedback sugestão problema bug"
        case .about: "sobre versão version"
        case .tasks: "tarefas agendadas agenda"
        case .disappearing: "temporárias somem apagar sumir timer"
        case .plans: "créditos planos pro business assinatura"
        case .discover: "descobrir comunidades públicas"
        case .moderation: "moderação denúncias report"
        case .invites: "convites amigos indicar business grátis"
        case .wallet: "carteira cartão pix pagamento"
        }
    }

    @ViewBuilder var view: some View {
        switch self {
        case .profile: ProfileSettings()
        case .preferences: PreferencesSettings()
        case .theme: ThemeSettings()
        case .security: SecuritySettings()
        case .devices: DevicesSettings()
        case .twoStep: TwoStepSettings()
        case .data: DataSettings()
        case .backup: BackupSettings()
        case .accessibility: AccessibilitySettings()
        case .legal: LegalSettings()
        case .feedback: FeedbackSettings()
        case .about: AboutSettings()
        case .tasks: TasksSettings()
        case .disappearing: DisappearingSettings()
        case .plans: PlansSettings()
        case .discover: DiscoverSettings()
        case .moderation: ModerationSettings()
        case .invites: InvitesSettings()
        case .wallet: WalletSettings()
        }
    }
}

// MARK: - Root (S49)

/// Ajustes as its own stack: opened from Você, closed by the X on any screen.
struct SettingsRoot: View {
    @Environment(\.dismiss) private var dismiss
    @State private var path: [SettingsScreen]

    init(start: SettingsScreen? = nil) {
        _path = State(initialValue: start.map { [$0] } ?? [])
    }

    var body: some View {
        NavigationStack(path: $path) {
            SettingsHome()
                .navigationDestination(for: SettingsScreen.self) { $0.view }
        }
        .environment(\.settingsClose, SettingsCloseAction { dismiss() })
    }
}

struct SettingsHome: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @FocusState private var searching: Bool

    private static let sections: [(LocalizedStringKey, [SettingsScreen])] = [
        ("Account", [.profile, .security, .devices, .backup, .data]),
        ("Preferences", [.preferences, .theme, .accessibility, .disappearing]),
        ("Zoen", [.plans, .invites, .wallet, .discover, .moderation, .tasks]),
        ("Help", [.feedback, .legal, .about]),
    ]

    private var results: [SettingsScreen] {
        let q = query.trimmingCharacters(in: .whitespaces).folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
        guard !q.isEmpty else { return [] }
        return SettingsScreen.allCases.filter { s in
            let hay = (String(localized: s.titleResource) + " " + s.keywords)
                .folding(options: [.diacriticInsensitive, .caseInsensitive], locale: nil)
            return hay.contains(q)
        }
    }

    var body: some View {
        SettingsPage(title: "Settings", showsBack: false) {
            searchField
            if query.trimmingCharacters(in: .whitespaces).isEmpty {
                // Identity comes from the account only, so nothing here contradicts "no account".
                if model.sync.account != nil, let me = model.me {
                    NavigationLink(value: SettingsScreen.profile) {
                        HStack(spacing: 14) {
                            PersonAvatar(persona: me, size: 52)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(me.name).font(.headline).foregroundStyle(Palette.textPrimary)
                                Text(verbatim: "@\(me.handle)").font(.subheadline).foregroundStyle(Palette.textSecondary)
                            }
                            Spacer()
                            Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.textTertiary)
                        }
                        .padding(16)
                        .background(SettingsStyle.card, in: .rect(cornerRadius: SettingsStyle.radius, style: .continuous))
                    }
                    .buttonStyle(PressScale())
                    .simultaneousGesture(TapGesture().onEnded { Haptics.tap() })
                    .accessibilityIdentifier("settingsProfileCard")
                }
                ForEach(Array(Self.sections.enumerated()), id: \.offset) { _, section in
                    SettingsGroup(header: section.0) {
                        ForEach(section.1) { NavRow(screen: $0) }
                    }
                }
            } else if results.isEmpty {
                SettingsEmpty(symbol: "magnifyingglass", title: "No results",
                              detail: "Nothing in Settings matches “\(query)”. Try another word, like “theme” or “password”.")
                    .accessibilityIdentifier("settingsNoResults")
            } else {
                SettingsGroup(header: "Results") {
                    ForEach(results) { NavRow(screen: $0) }
                }
            }
        }
    }

    private var searchField: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.textSecondary)
            TextField("Search settings", text: $query)
                .focused($searching)
                .autocorrectionDisabled()
                .accessibilityIdentifier("settingsSearch")
            if !query.isEmpty {
                Button { query = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.textTertiary)
                        .frame(width: 44, height: 44)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Clear search"))
            }
        }
        .padding(.leading, 16)
        .frame(height: 48)
        .background(SettingsStyle.card, in: .capsule)
        .overlay(Capsule().strokeBorder(SettingsStyle.controlOutline, lineWidth: 1))
    }
}

extension SettingsScreen {
    /// Same key as `title`, for searching in the current language.
    var titleResource: LocalizedStringResource {
        switch self {
        case .profile: "Profile"
        case .preferences: "Preferences"
        case .theme: "Theme"
        case .security: "Security"
        case .devices: "Devices"
        case .twoStep: "Two-step verification"
        case .data: "Your data"
        case .backup: "Backup and recovery"
        case .accessibility: "Accessibility"
        case .legal: "Terms and privacy"
        case .feedback: "Feedback"
        case .about: "About"
        case .tasks: "Tasks"
        case .disappearing: "Disappearing messages"
        case .plans: "Credits and plans"
        case .discover: "Discover"
        case .moderation: "Moderation"
        case .invites: "Invites"
        case .wallet: "Wallet"
        }
    }
}

/// A row that pushes another Ajustes screen.
struct NavRow: View {
    let screen: SettingsScreen
    var value: String? = nil
    var body: some View {
        NavigationLink(value: screen) {
            HStack(spacing: 12) {
                Image(systemName: screen.symbol)
                    .font(.system(size: 16, weight: .medium))
                    .foregroundStyle(Palette.textSecondary)
                    .frame(width: 22)
                Text(screen.title).font(.body.weight(.medium)).foregroundStyle(Palette.textPrimary)
                Spacer(minLength: 8)
                if let value { Text(value).font(.subheadline).foregroundStyle(Palette.textSecondary) }
                Image(systemName: "chevron.right").font(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.textTertiary)
            }
            .padding(.horizontal, 16)
            .frame(minHeight: SettingsStyle.rowHeight)
            .contentShape(.rect)
        }
        .buttonStyle(RowPress())
        .simultaneousGesture(TapGesture().onEnded { Haptics.tap() })
        .accessibilityIdentifier("settings.\(screen.rawValue)")
    }
}

// MARK: - S50 Perfil

struct ProfileSettings: View {
    @Environment(AppModel.self) private var model
    @State private var editing = false

    var body: some View {
        SettingsPage(title: "Profile") {
            if model.sync.account != nil, let me = model.me {
                VStack(spacing: 8) {
                    PersonAvatar(persona: me, size: 72)
                    Text(me.name).font(.title3.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
            }
            if let acct = model.sync.account {
                SettingsGroup {
                    SettingsRow(symbol: "person", title: "Name", value: acct.name, id: "profileName") { editing = true }
                    SettingsRow(symbol: "at", title: "Username", value: "@\(acct.handle)", id: "profileHandle") { editing = true }
                }
                SettingsCaption("People find you by your @. Your name and photo travel encrypted, only to people you talk to.")
                    .sheet(isPresented: $editing) { ProfileEditor(account: acct) }
            } else {
                SettingsEmpty(symbol: "person.crop.circle", title: "No account on this device",
                              detail: "Create your account to choose a name and an @ that friends can find.")
            }
        }
    }
}

// MARK: - S51 Preferências

struct PreferencesSettings: View {
    @AppStorage(AppAppearance.key) private var appearance = AppAppearance.auto.rawValue
    @Environment(AppModel.self) private var model

    var body: some View {
        SettingsPage(title: "Preferences") {
            SettingsGroup {
                NavRow(screen: .theme, value: AppAppearance(rawValue: appearance)?.label ?? AppAppearance.auto.label)
                SettingsRow(symbol: "character.bubble", title: "Language",
                            value: Locale.current.localizedString(forLanguageCode: Locale.current.language.languageCode?.identifier ?? "pt")?.capitalized,
                            id: "prefsLanguage") { openSystemSettings() }
                NavRow(screen: .data, value: model.stats.map { String(localized: "\(Int($0.items)) items") })
                NavRow(screen: .accessibility)
            }
            SettingsCaption("The language follows your device. Tap it to change it in the device settings.")
        }
    }
}

/// The app's light/dark choice (read by the app root).
enum AppAppearance: String, CaseIterable {
    case auto, light, dark
    static let key = "RodaAppearance"
    var label: String {
        switch self {
        case .auto: String(localized: "Automatic")
        case .light: String(localized: "Light")
        case .dark: String(localized: "Dark")
        }
    }
    var scheme: ColorScheme? {
        switch self {
        case .auto: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

// MARK: - S52 Tema

struct ThemeSettings: View {
    @AppStorage(AppAppearance.key) private var appearance = AppAppearance.auto.rawValue

    var body: some View {
        SettingsPage(title: "Theme") {
            SettingsCaption("The app and the mini-apps change together.")
            SettingsGroup {
                ForEach(AppAppearance.allCases, id: \.self) { a in
                    SettingsRadioRow(title: LocalizedStringKey(a.label),
                                     subtitle: a == .auto ? "Follows your device" : nil,
                                     selected: (AppAppearance(rawValue: appearance) ?? .auto) == a,
                                     id: "theme.\(a.rawValue)") {
                        appearance = a.rawValue
                    }
                }
            }
        }
    }
}

// MARK: - S53 Segurança

struct SecuritySettings: View {
    @Environment(AppModel.self) private var model
    @State private var devices: [DeviceDto]? = nil

    var body: some View {
        let backup = model.core.backupStatus()
        SettingsPage(title: "Security") {
            SettingsGroup {
                NavRow(screen: .twoStep, value: String(localized: "Off"))
                NavRow(screen: .backup, value: backup.enabled ? String(localized: "On") : String(localized: "Off"))
                NavRow(screen: .devices, value: devices.map { "\($0.filter { !$0.revoked }.count)" })
            }
            SettingsCaption("Your messages are end-to-end encrypted. Only your devices hold the keys.")
            if model.sync.account != nil {
                SettingsGroup {
                    Toggle("Ocultar visto por último", isOn: Binding(
                        get: { model.sync.hideLastSeen },
                        set: { model.sync.setHideLastSeen($0) }
                    ))
                    .padding(.horizontal, 16)
                    .frame(minHeight: SettingsStyle.rowHeight)
                    .accessibilityIdentifier("settings.hideLastSeen")
                }
                SettingsCaption("Quando ativado, outras pessoas não veem quando você esteve online pela última vez. Seu status online continua visível.")
            }
        }
        .task { devices = try? await model.core.devices() }
    }
}

// MARK: - S54 Aparelhos

struct DevicesSettings: View {
    @Environment(AppModel.self) private var model
    @State private var devices: [DeviceDto]? = nil
    @State private var failed: String? = nil

    var body: some View {
        SettingsPage(title: "Devices") {
            if model.sync.account == nil {
                SettingsEmpty(symbol: "laptopcomputer.and.iphone", title: "No account on this device",
                              detail: "Once you create your account, the devices signed in to it show up here.")
            } else if let devices {
                let active = devices.filter { !$0.revoked }
                SettingsCaption("Devices signed in to your account.")
                SettingsGroup {
                    ForEach(active, id: \.deviceId) { d in
                        SettingsRow(symbol: d.thisDevice ? thisDeviceSymbol : "desktopcomputer",
                                    title: d.thisDevice ? "This device" : "Another device",
                                    subtitle: String(localized: "Code \(String(d.deviceId.prefix(8)))"),
                                    value: d.thisDevice ? String(localized: "Now") : nil)
                    }
                }
                SettingsCaption("To add one, open Zoen on the new device and choose to sign in with this one.")
            } else if let failed {
                SettingsEmpty(symbol: "wifi.slash", title: "Couldn't load your devices", detail: LocalizedStringKey(failed))
            } else {
                ProgressView().frame(maxWidth: .infinity).padding(.vertical, 40)
            }
        }
        .task {
            guard model.sync.account != nil else { return }
            do { devices = try await model.core.devices() } catch {
                failed = (error as? CoreError)?.message ?? error.localizedDescription
            }
        }
    }

    private var thisDeviceSymbol: String {
        #if os(macOS)
        "laptopcomputer"
        #else
        UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        #endif
    }
}

// MARK: - S55 Duas etapas

struct TwoStepSettings: View {
    var body: some View {
        SettingsPage(title: "Two-step verification") {
            SettingsCaption("A code from your phone will confirm it's you when a new device signs in.")
            SettingsGroup {
                SettingsRow(symbol: "key", title: "Authenticator app", subtitle: String(localized: "Recommended · works without signal"), value: String(localized: "Soon"))
                SettingsRow(symbol: "message", title: "Text message", subtitle: String(localized: "Code by text message"), value: String(localized: "Soon"))
            }
            .opacity(0.6)
            SettingsCaption("Not available yet. Today a new device only gets in with approval from one that's already signed in.")
        }
    }
}

// MARK: - S59 Seus dados

struct DataSettings: View {
    @Environment(AppModel.self) private var model
    @State private var confirmErase = false

    var body: some View {
        SettingsPage(title: "Your data") {
            if let s = model.stats {
                SettingsGroup {
                    SettingsRow(symbol: "bubble.left.and.bubble.right", title: "Chats", value: "\(s.spaces)")
                    SettingsRow(symbol: "square.stack.3d.up", title: "Items", value: "\(s.items)")
                    SettingsRow(symbol: "person.2", title: "People and agents", value: "\(s.identities)")
                }
            }
            SettingsGroup {
                NavRow(screen: .backup, value: model.core.backupStatus().enabled ? String(localized: "On") : String(localized: "Off"))
            }
            if model.sync.account != nil {
                SettingsGroup {
                    SettingsRow(symbol: "trash", title: "Erase data on this device", tint: Palette.danger, chevron: false, id: "eraseThisDevice") {
                        Haptics.warning()
                        confirmErase = true
                    }
                }
                SettingsCaption("Erasing here doesn't delete your account. Your chats come back when you sign in again with another device.")
            }
        }
        .confirmationDialog("Erase data on this device?", isPresented: $confirmErase, titleVisibility: .visible) {
            Button("Erase", role: .destructive) {
                do { try model.sync.eraseDevice() } catch {
                    model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
                }
            }
        } message: {
            Text("Chats and keys on this device are deleted. Without a backup or another signed-in device, they can't be recovered.")
        }
    }
}

// MARK: - S61 Cópia e recuperação

struct BackupSettings: View {
    @Environment(AppModel.self) private var model
    @State private var status: BackupStatusDto? = nil
    @State private var askPassword = false
    @State private var busy = false

    var body: some View {
        let s = status ?? model.core.backupStatus()
        SettingsPage(title: "Backup and recovery") {
            SettingsGroup {
                if s.enabled {
                    SettingsRow(symbol: "checkmark.icloud", title: "Backup is on",
                                subtitle: s.lastBackupMs > 0
                                    ? String(localized: "Last backup: \(Date(timeIntervalSince1970: Double(s.lastBackupMs) / 1000).formatted(.relative(presentation: .named)))")
                                    : String(localized: "No backup yet"))
                    SettingsRow(symbol: "arrow.triangle.2.circlepath.icloud", title: "Back up now", id: "backupNow") { Task { await backupNow() } }
                } else {
                    SettingsRow(symbol: "icloud.slash", title: "Backup is off",
                                subtitle: String(localized: "Your chats live only on your devices"))
                    if model.sync.account != nil {
                        SettingsRow(symbol: "lock", title: "Turn on with a password", id: "backupTurnOn") { askPassword = true }
                    }
                }
            }
            SettingsGroup {
                SettingsRow(symbol: "clock.arrow.circlepath", title: "Restore from a backup",
                            subtitle: String(localized: "Not available on this device yet"), chevron: false, id: "backupRestore")
                    .opacity(0.45)
                    .accessibilityHint(Text("Unavailable"))
            }
            SettingsCaption("Restoring happens when you sign in on a new device: choose “I already have an account” and type your backup password.")
        }
        .sheet(isPresented: $askPassword) { BackupPasswordSheet { status = $0 } }
        .disabled(busy)
    }

    private func backupNow() async {
        busy = true
        defer { busy = false }
        do {
            status = try await model.core.backupNow(vault: model.sync.vault)
            Haptics.commit()
        } catch {
            Haptics.warning()
            model.show(.init(kind: .error, text: (error as? CoreError)?.message ?? error.localizedDescription))
        }
    }
}

private struct BackupPasswordSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let done: (BackupStatusDto) -> Void
    @State private var password = ""
    @State private var busy = false
    @State private var error: String? = nil

    var body: some View {
        NavigationStack {
            SettingsPage(title: "Backup password", showsBack: false) {
                SettingsCaption("Pick a password with at least 8 characters. We can't recover it for you: after 10 wrong tries the backup locks for good.")
                SecureField("Password", text: $password)
                    .textContentType(.newPassword)
                    .padding(.horizontal, 16)
                    .frame(height: 50)
                    .background(SettingsStyle.card, in: .rect(cornerRadius: 14, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(SettingsStyle.controlOutline, lineWidth: 1))
                    .accessibilityIdentifier("backupPassword")
                if let error { Text(error).font(.footnote).foregroundStyle(Palette.danger) }
            } footer: {
                SettingsPrimaryButton(title: "Turn on backup", id: "backupConfirm", busy: busy) {
                    Task { await turnOn() }
                }
                .disabled(password.count < 8 || busy)
            }
            .environment(\.settingsClose, SettingsCloseAction { dismiss() })
        }
        .presentationDetents([.medium, .large])
    }

    private func turnOn() async {
        busy = true
        defer { busy = false }
        do {
            let s = try await model.core.backupTurnOnPassword(password: password, vault: model.sync.vault)
            Haptics.commit()
            done(s)
            dismiss()
        } catch {
            Haptics.warning()
            self.error = (error as? CoreError)?.message ?? error.localizedDescription
        }
    }
}

// MARK: - S62 Acessibilidade

struct AccessibilitySettings: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.colorSchemeContrast) private var contrast
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        SettingsPage(title: "Accessibility") {
            SettingsGroup {
                SettingsRow(symbol: "move.3d", title: "Reduce motion", value: onOff(reduceMotion))
                SettingsRow(symbol: "square.on.square", title: "Reduce transparency", value: onOff(reduceTransparency))
                SettingsRow(symbol: "circle.righthalf.filled", title: "Increase contrast", value: onOff(contrast == .increased))
                SettingsRow(symbol: "textformat.size", title: "Text size", value: typeSize > .large ? String(localized: "Larger") : (typeSize < .large ? String(localized: "Smaller") : String(localized: "Default")))
            }
            SettingsGroup {
                SettingsRow(symbol: "gear", title: "Change in device settings", id: "a11yOpenSettings") { openSystemSettings() }
            }
            SettingsCaption("Zoen follows your device. Whatever you change there also applies to the mini-apps.")
        }
    }

    private func onOff(_ on: Bool) -> String { on ? String(localized: "On") : String(localized: "Off") }
}

@MainActor
func openSystemSettings() {
    #if os(iOS)
    if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
    #endif
}

// MARK: - S63 Termos e privacidade

struct LegalSettings: View {
    var body: some View {
        SettingsPage(title: "Terms and privacy") {
            SettingsEmpty(symbol: "doc.text", title: "Coming before launch",
                          detail: "The terms of use and the privacy policy will be here before Zoen opens to everyone.")
            SettingsCaption("Meanwhile: your messages are end-to-end encrypted, and Zoen's servers can't read them.")
        }
    }
}

// MARK: - S64 Feedback

struct FeedbackSettings: View {
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        SettingsPage(title: "Feedback") {
            SettingsCaption("Tell us what we can do better.")
            TextEditor(text: $text)
                .focused($focused)
                .scrollContentBackground(.hidden)
                .padding(12)
                .frame(minHeight: 160)
                .background(SettingsStyle.card, in: .rect(cornerRadius: SettingsStyle.radius, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: SettingsStyle.radius, style: .continuous).strokeBorder(SettingsStyle.controlOutline, lineWidth: 1))
                .overlay(alignment: .topLeading) {
                    if text.isEmpty {
                        Text("What would make Zoen better for you?")
                            .foregroundStyle(Palette.textTertiary)
                            .padding(.horizontal, 17).padding(.vertical, 20)
                            .allowsHitTesting(false)
                    }
                }
                .accessibilityIdentifier("feedbackText")
        } footer: {
            ShareLink(item: text) {
                Text("Send")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.background)
                    .frame(maxWidth: .infinity, minHeight: 50)
                    .background(Palette.textPrimary.opacity(text.isEmpty ? 0.35 : 1), in: .capsule)
            }
            .buttonStyle(PressScale())
            .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .accessibilityIdentifier("feedbackSend")
        }
        .onAppear { focused = true }
    }
}

// MARK: - S65 Sobre

struct AboutSettings: View {
    private var version: String {
        let v = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0"
        let b = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "1"
        return "\(v) (\(b))"
    }

    var body: some View {
        SettingsPage(title: "About") {
            SettingsGroup {
                SettingsRow(symbol: "info.circle", title: "Version", value: version)
                NavRow(screen: .legal)
            }
            SettingsCaption("Zoen: friends, communities, apps and agents in one place.")
        }
    }
}

// MARK: - S67 Tarefas

struct TasksSettings: View {
    var body: some View {
        SettingsPage(title: "Tasks") {
            SettingsEmpty(symbol: "calendar.badge.clock", title: "No scheduled tasks",
                          detail: "Ask an agent to do something later, and it shows up here.")
        }
    }
}

// MARK: - S39 Mensagens temporárias

struct DisappearingSettings: View {
    var body: some View {
        SettingsPage(title: "Disappearing messages") {
            SettingsCaption("New messages in a Space vanish after the time you pick, for everyone.")
            SettingsGroup {
                SettingsRadioRow(title: "Off", subtitle: "Soon", selected: false, id: "disappear.off") {}
                SettingsRadioRow(title: "24 hours", subtitle: "Soon", selected: false, id: "disappear.24h") {}
                SettingsRadioRow(title: "7 days", subtitle: "Soon", selected: false, id: "disappear.7d") {}
                SettingsRadioRow(title: "90 days", subtitle: "Soon", selected: false, id: "disappear.90d") {}
            }
            .disabled(true)
            .accessibilityHint(Text("Unavailable"))
            SettingsCaption("Not available yet: today every message stays. When it arrives, you'll pick the time here for each Space, and older messages won't change.")
        }
    }
}

// MARK: - S43 Créditos e planos

struct PlansSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let mine = model.sync.account == nil ? [] : model.agents.filter { $0.persona.isMine }
        let limit = mine.compactMap(\.budgetLimitCents).reduce(0, +)
        let spent = mine.compactMap(\.budgetSpentCents).reduce(0, +)
        SettingsPage(title: "Credits and plans") {
            if limit > 0 {
                SettingsMeter(label: "Your agents this month",
                              value: money(spent),
                              progress: Double(spent) / Double(max(limit, 1)),
                              detail: String(localized: "of \(money(limit)) in budgets you set"))
            } else {
                SettingsEmpty(symbol: "sparkles", title: "No agent spending yet",
                              detail: "When your agents work for you, what they spend this month shows up here.")
            }
            SettingsGroup(header: "Your plan") {
                SettingsRow(symbol: "person", title: "Free", subtitle: String(localized: "Your current plan"), value: String(localized: "Active"))
            }
            SettingsGroup {
                NavRow(screen: .invites, value: String(localized: "Business free"))
            }
            SettingsCaption("Paid plans aren't on sale yet. Meanwhile, every 5 active friends you invite give you 1 month of Business.")
        }
    }

    private func money(_ cents: Int64) -> String {
        (Double(cents) / 100).formatted(.currency(code: Locale.current.currency?.identifier ?? "BRL"))
    }
}

// MARK: - S44 Descobrir

struct DiscoverSettings: View {
    var body: some View {
        SettingsPage(title: "Discover") {
            SettingsEmpty(symbol: "safari", title: "No public communities yet",
                          detail: "When people open their Spaces to everyone, you'll find them here. For now, join with a friend's invite link.")
        }
    }
}

// MARK: - S46 Moderação

struct ModerationSettings: View {
    var body: some View {
        SettingsPage(title: "Moderation") {
            SettingsEmpty(symbol: "flag", title: "No reports",
                          detail: "Reports from your Spaces show up here. The reported person never knows who reported.")
        }
    }
}

// MARK: - S47 Convites

struct InvitesSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        SettingsPage(title: "Invites") {
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: "gift").font(.system(size: 22, weight: .medium)).foregroundStyle(Palette.textPrimary)
                Text("1 month of Business for every 5 friends").font(.title3.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                Text("Invite friends with your link. Every 5 who become active give you 1 month of the Business plan. No limit.")
                    .font(.subheadline).foregroundStyle(Palette.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(SettingsStyle.card, in: .rect(cornerRadius: SettingsStyle.radius, style: .continuous))
            SettingsCaption("Friends who joined with your link show up here as they become active.")
        } footer: {
            if let link = model.core.friendInviteLink(), let url = URL(string: link) {
                ShareLink(item: url, message: Text("Come talk to me on Zoen")) {
                    Text("Invite someone")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(Palette.background)
                        .frame(maxWidth: .infinity, minHeight: 50)
                        .background(Palette.textPrimary, in: .capsule)
                }
                .buttonStyle(PressScale())
                .simultaneousGesture(TapGesture().onEnded { Haptics.tap() })
                .accessibilityIdentifier("inviteShare")
            }
        }
    }
}

// MARK: - S68 Carteira

struct WalletSettings: View {
    var body: some View {
        SettingsPage(title: "Wallet") {
            SettingsEmpty(symbol: "creditcard", title: "No payment methods",
                          detail: "Cards and Pix arrive with paid plans and mini-app purchases.")
            SettingsCaption("When they do, card details stay with the payment processor. Zoen never stores the number.")
        }
    }
}
