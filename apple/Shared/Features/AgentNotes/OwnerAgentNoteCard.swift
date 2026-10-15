import SwiftUI
import RodaCore

/// A note your own agent sends you: it needs your say before answering someone, or it
/// wants to create a new agent. The core decides whether a message is one of these
/// (`RodaEngine.ownerAgentNote`, which checks the author is your agent with a verified
/// ownership proof); the app only renders the JSON it hands back.
enum OwnerAgentNote: Equatable {
    case ask(space: String, from: String, draft: String)
    case proposal(space: String, name: String, handle: String)
    /// A client booked, cancelled, or asked to cancel (`kind`: new / cancelled / cancel_request).
    case booking(space: String, kind: String, client: String, service: String, when: String)

    init?(json: String) {
        guard let data = json.data(using: .utf8),
              let root = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        else { return nil }
        if let a = root["ask_owner"] as? [String: Any] {
            self = .ask(space: a["space"] as? String ?? "",
                        from: a["from"] as? String ?? "",
                        draft: a["draft"] as? String ?? "")
        } else if let p = root["agent_proposal"] as? [String: Any] {
            let spec = p["spec"] as? [String: Any] ?? [:]
            self = .proposal(space: p["space"] as? String ?? "",
                             name: spec["name"] as? String ?? "",
                             handle: spec["handle"] as? String ?? "")
        } else if let b = root["booking"] as? [String: Any] {
            self = .booking(space: b["space"] as? String ?? "",
                            kind: b["kind"] as? String ?? "new",
                            client: b["client"] as? String ?? "",
                            service: b["service"] as? String ?? "",
                            when: b["when"] as? String ?? "")
        } else {
            return nil
        }
    }

    var space: String {
        switch self { case .ask(let s, _, _), .proposal(let s, _, _), .booking(let s, _, _, _, _): s }
    }
}

extension AppModel {
    /// The card for this message, if it is a note from your own agent. Only agent messages
    /// that are one fenced JSON block ask the core. A card is cached per message; a "no"
    /// is not, because the agent's verified profile may sync after its message.
    func ownerAgentNote(_ entry: TimelineEntry, text: String) -> OwnerAgentNote? {
        guard entry.author.kind == .agent, !entry.author.isMe,
              text.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("```json")
        else { return nil }
        if let hit = ownerNoteCache[entry.id] { return hit }
        let note = core.ownerAgentNote(authorId: entry.author.id, text: text).flatMap(OwnerAgentNote.init(json:))
        if let note { ownerNoteCache[entry.id] = note }
        return note
    }

    func personName(_ id: String) -> String? {
        spaces.lazy.flatMap(\.members).first { $0.id == id }?.name
    }
}

struct OwnerAgentNoteCard: View {
    let agent: Persona
    let note: OwnerAgentNote
    /// Arrived just now: the card enters once. Older notes are already there.
    let fresh: Bool
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            content
            Button {
                Haptics.tap()
                if !note.space.isEmpty { model.go(.space(note.space)) }
            } label: {
                Text("Abrir conversa")
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 11)
                    .background(Palette.action, in: .rect(cornerRadius: 12, style: .continuous))
                    .foregroundStyle(Palette.onAction)
            }
            .buttonStyle(NotePressStyle())
            .disabled(note.space.isEmpty)
            .accessibilityHint("Abre a conversa onde isso aconteceu")
        }
        .padding(16)
        .frame(maxWidth: 340, alignment: .leading)
        // 16 inner padding + 12 button radius = 28.
        .background(Palette.surface, in: .rect(cornerRadius: 28, style: .continuous))
        .shadow(color: .black.opacity(0.06), radius: 1, y: 0.5)
        .shadow(color: .black.opacity(0.08), radius: 12, y: 4)
        .accessibilityElement(children: .contain)
        .opacity(shown ? 1 : 0)
        .offset(y: shown || reduceMotion ? 0 : 12)
        .blur(radius: shown || reduceMotion ? 0 : 4)
        .onAppear {
            guard fresh, !shown else { shown = true; return }
            withAnimation(.easeOut(duration: reduceMotion ? 0.2 : 0.3)) { shown = true }
            Haptics.warning()
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            ContactAvatar(persona: agent, size: 28)
            VStack(alignment: .leading, spacing: 1) {
                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.textPrimary)
                Text(agent.name).font(.caption).foregroundStyle(Palette.textSecondary)
            }
        }
    }

    private var title: LocalizedStringKey {
        switch note {
        case .ask: "Seu agente precisa de você"
        case .proposal: "Seu agente quer criar um agente"
        case .booking(_, "cancelled", _, _, _): "Agendamento cancelado"
        case .booking(_, "cancel_request", _, _, _): "Pedido de cancelamento"
        case .booking: "Novo agendamento"
        }
    }

    @ViewBuilder private var content: some View {
        switch note {
        case .ask(_, let from, let draft):
            let who = model.personName(from) ?? String(localized: "alguém")
            Text("Antes de responder a \(who), ele quer que você veja a resposta.")
                .font(.callout).foregroundStyle(Palette.textPrimary)
            if !draft.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Resposta sugerida").font(.caption.weight(.semibold)).foregroundStyle(Palette.textSecondary)
                    Text(draft).font(.callout).foregroundStyle(Palette.textPrimary).lineLimit(6)
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.surfaceMuted, in: .rect(cornerRadius: 12, style: .continuous))
            }
        case .proposal(_, let name, let handle):
            HStack(spacing: 8) {
                Text(name.isEmpty ? handle : name).font(.callout.weight(.semibold))
                if !handle.isEmpty { Text("@\(handle)").font(.callout).foregroundStyle(Palette.textSecondary) }
            }
            .foregroundStyle(Palette.textPrimary)
            Text("Ele só começa a funcionar depois que você aprovar.")
                .font(.callout).foregroundStyle(Palette.textSecondary)
        case .booking(_, let kind, let client, let service, let when):
            let who = client.isEmpty ? String(localized: "Alguém") : client
            Text(kind == "cancelled" ? "\(who) cancelou \(service), \(when)."
                 : kind == "cancel_request" ? "\(who) pediu para cancelar \(service), \(when)."
                 : "\(who) marcou \(service), \(when).")
                .font(.callout).foregroundStyle(Palette.textPrimary)
        }
    }
}

/// Press scales to 0.96 over 150 ms, ease-out; disabled buttons never scale.
private struct NotePressStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed && enabled && !reduceMotion ? 0.96 : 1)
            .opacity(enabled ? 1 : 0.5)
            .animation(.easeOut(duration: 0.15), value: configuration.isPressed)
    }
}
