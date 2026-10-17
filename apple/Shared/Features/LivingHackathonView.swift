import SwiftUI
import RodaCore

/// A native participation flow. Core owns membership, deadlines, votes and results.
struct LivingHackathonView: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    var initialSection: String? = nil
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var saved = false
    @State private var home: LivingSpaceProjection?
    @State private var failure: String?
    @State private var title = ""
    @State private var deadline = Date().addingTimeInterval(7 * 86400)
    @State private var teamName = ""
    @State private var skills = ""
    @State private var projectURL = ""
    @State private var selectedFiles: Set<String> = []
    @State private var milestone = ""
    @State private var milestoneDate = Date()
    @State private var score = 0
    @State private var clock = Date()

    private var me: String { model.me?.id ?? "" }
    private var admin: Bool { home?.canManage == true }
    private var hackathon: LivingHackathon? { home?.hackathon }
    private var myTeam: LivingHackathon.Team? { hackathon?.teams.first { $0.members.contains(me) } }
    private var files: [ItemDetail] { model.core.items().filter { (home?.fileItems ?? []).contains($0.id) } }

    var body: some View {
        ScrollViewReader { proxy in
        Form {
            if let hackathon {
                overview(hackathon).id("overview")
                teams(hackathon).id("teams")
                schedule(hackathon)
                submission(hackathon).id("submission")
                judging(hackathon)
                winners(hackathon).id("results")
            } else if admin {
                Section("Create hackathon") {
                    TextField("Title", text: $title)
                    DatePicker("Submission deadline", selection: $deadline)
                    Button("Create hackathon", systemImage: "trophy") {
                        command(["action": "hackathon_create", "title": title, "deadline_ms": Int64(deadline.timeIntervalSince1970 * 1000)])
                    }.disabled(title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            } else { ContentUnavailableView("No hackathon yet", systemImage: "trophy") }
            if saved { Section { Label("Saved", systemImage: "checkmark.circle.fill").foregroundStyle(.secondary) }.transition(.opacity) }
            if let failure { Section { Text(failure).foregroundStyle(.red) } }
        }
        .task {
            reload()
            if let initialSection {
                try? await Task.sleep(for: .milliseconds(350))
                proxy.scrollTo(initialSection, anchor: .top)
            }
        }
        }
        .navigationTitle("Hackathon")
        .task(id: model.revision) { reload() }
        .onReceive(Timer.publish(every: 1, on: .main, in: .common).autoconnect()) { clock = $0 }
        .accessibilityIdentifier("living-space.hackathon")
    }

    private func overview(_ h: LivingHackathon) -> some View {
        Section {
            Label(h.title, systemImage: "trophy").font(.title2.bold())
            LabeledContent("Submission deadline") { Text(h.deadline, format: .dateTime.month().day().hour().minute()) }
            if !h.isClosed {
                HStack {
                    Text("Time remaining")
                    Spacer()
                    Text(timerInterval: clock...h.deadline, countsDown: true).monospacedDigit()
                }.accessibilityLabel("Countdown to submission deadline")
            } else { Label("Submissions closed", systemImage: "lock") }
            Text(h.registered.count == 1 ? String(localized: "1 participant") : String(localized: "\(h.registered.count) participants"))
                .foregroundStyle(.secondary)
            if h.registered.contains(me) { Label("You're registered", systemImage: "checkmark.circle.fill").foregroundStyle(.secondary) }
            TextField("What would you like to work on?", text: $skills, axis: .vertical).lineLimit(2...4)
            Button(h.registered.contains(me) ? String(localized: "Update interests") : String(localized: "Register"), systemImage: "person.badge.plus") {
                command(["action": "register", "skills": skills])
            }.buttonStyle(.glass).disabled(h.isClosed)
            if !h.isClosed && h.deadline <= clock {
                Text("Awaiting administrator to close submissions").font(.caption).foregroundStyle(.secondary)
            }
        } header: { Text("Overview") }
    }

    private func teams(_ h: LivingHackathon) -> some View {
        Section("Find teammates") {
            let available = h.registered.filter { identity in !h.teams.contains { $0.members.contains(identity) } }
            ForEach(available, id: \.self) { identity in
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.core.personas().first { $0.id == identity }?.name ?? String(localized: "Community member")).font(.headline)
                    if let profile = h.profiles?[identity], !profile.isEmpty { Text(profile).font(.subheadline).foregroundStyle(.secondary) }
                }
            }
            if h.teams.isEmpty { Text("No teams yet. Start one and invite your teammates.").foregroundStyle(.secondary) }
            ForEach(h.teams) { team in
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text(team.name).font(.headline)
                        Spacer()
                        Text("\(team.members.count) members").font(.caption).foregroundStyle(.secondary)
                    }
                    ForEach(Array((h.pendingTeamRequests ?? []).filter { $0.teamId == team.id }.enumerated()), id: \.offset) { _, request in
                        Text(model.core.personas().first { $0.id == request.identityId }?.name ?? String(localized: "Community member")).font(.caption)
                        Button("Accept teammate", systemImage: "person.badge.plus") {
                            command(["action": "team_join", "team_id": team.id, "identity_id": request.identityId])
                        }.disabled(h.isClosed)
                    }
                    if team.members.contains(me) {
                        if let conversation = team.space {
                            NavigationLink { LivingSpaceConversation(spaceId: conversation) } label: {
                                Label("Encrypted team conversation", systemImage: "lock.bubble")
                            }
                        }
                        Text("People can request to join your team here. Review their requests above.")
                            .font(.caption).foregroundStyle(.secondary)
                    } else if myTeam == nil {
                        Button("Request to join", systemImage: "person.crop.circle.badge.plus") {
                            command(["action": "team_join", "team_id": team.id])
                        }.disabled(h.isClosed)
                    }
                }.padding(.vertical, 6)
            }
            if myTeam == nil && h.registered.contains(me) {
                TextField("Team name", text: $teamName)
                Button("Create team", systemImage: "person.3") { command(["action": "team_create", "name": teamName]) }
                    .disabled(h.isClosed || teamName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
    }

    private func schedule(_ h: LivingHackathon) -> some View {
        Section("Schedule") {
            ForEach(Array(h.schedule.enumerated()), id: \.offset) { _, event in
                LabeledContent(event.title) { Text(event.date, format: .dateTime.month().day().hour().minute()) }
            }
            if h.schedule.isEmpty { Text("No milestones yet.").foregroundStyle(.secondary) }
            if admin {
                TextField("Milestone", text: $milestone)
                DatePicker("When", selection: $milestoneDate)
                Button("Add milestone", systemImage: "calendar.badge.plus") {
                    command(["action": "schedule_event", "title": milestone, "at_ms": Int64(milestoneDate.timeIntervalSince1970 * 1000)])
                }.disabled(milestone.isEmpty)
            }
        }
    }

    private func submission(_ h: LivingHackathon) -> some View {
        Section("Project") {
            if let team = myTeam {
                if let url = team.projectUrl, let link = URL(string: url), ["https", "http"].contains(link.scheme?.lowercased() ?? "") {
                    Link("Submitted project", destination: link)
                }
                TextField("Project URL", text: $projectURL)
                    .textContentType(.URL)
                ForEach(files, id: \.id) { file in
                    Toggle(file.title, isOn: Binding(get: { selectedFiles.contains(file.id) }, set: { on in
                        if on { selectedFiles.insert(file.id) } else { selectedFiles.remove(file.id) }
                    }))
                }
                Button("Submit project", systemImage: "paperplane") {
                    command(["action": "project_submit", "team_id": team.id, "url": projectURL, "file_ids": Array(selectedFiles)])
                }.disabled(h.isClosed || (projectURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && selectedFiles.isEmpty))
                ForEach(team.files, id: \.self) { id in
                    Button("Open submitted file", systemImage: "doc") { model.push(.item(id)) }
                }
            } else { Text("Join a team to submit a project.").foregroundStyle(.secondary) }
            Label("Demo mini-app · Coming later", systemImage: "lock").font(.caption).foregroundStyle(.secondary)
        }
    }

    private func judging(_ h: LivingHackathon) -> some View {
        Section("Judging & voting") {
            ForEach(h.teams) { team in
                let noSubmission = team.projectUrl == nil && team.files.isEmpty
                let cannotVote = h.finalized || !h.isClosed || !h.registered.contains(me) || team.members.contains(me) || noSubmission
                let cannotJudge = h.finalized || !h.isClosed || team.members.contains(me) || noSubmission
                VStack(alignment: .leading, spacing: 10) {
                    LabeledContent(team.name, value: "\(team.votes) votes · \(team.score) points")
                    Button("Vote", systemImage: "hand.thumbsup") { command(["action": "vote", "team_id": team.id]) }
                        .disabled(cannotVote)
                        .opacity(cannotVote ? 0.45 : 1)
                    if let raw = team.projectUrl, let url = URL(string: raw), ["https", "http"].contains(url.scheme?.lowercased() ?? "") {
                        Link("View project", destination: url)
                    }
                    ForEach(team.files, id: \.self) { id in
                        Button("Open project file", systemImage: "doc") { model.push(.item(id)) }
                    }
                    if h.canJudge == true || admin {
                        Stepper("Score: \(score)", value: $score, in: 0...100).disabled(cannotJudge)
                        Button("Submit judge score", systemImage: "checkmark.seal") { command(["action": "judge", "team_id": team.id, "score": score]) }
                            .disabled(cannotJudge)
                            .opacity(cannotJudge ? 0.45 : 1)
                    }
                }.padding(.vertical, 6)
            }
            if admin {
                Menu("Assign judge", systemImage: "checkmark.seal") {
                    ForEach(model.space(spaceId)?.members.filter { $0.kind == .person } ?? [], id: \.id) { person in
                        Button(person.name) { command(["action": "set_judge", "identity_id": person.id]) }
                    }
                }
            }
        }
    }

    private func winners(_ h: LivingHackathon) -> some View {
        Section("Winners") {
            ForEach(h.teams.filter { h.winners.contains($0.id) }) { team in
                Label(team.name, systemImage: "trophy.fill").font(.headline)
            }
            if h.winners.isEmpty { Text("Results have not been published.").foregroundStyle(.secondary) }
            if admin && !h.isClosed {
                Button("Close submissions and open voting", systemImage: "lock") { command(["action": "submissions_close"]) }
            }
            if admin && !h.finalized {
                Button("Publish winners announcement", systemImage: "megaphone") { command(["action": "finalize"]) }
                    .disabled(!h.isClosed)
            }
            if h.finalized { Label("Results published to announcements", systemImage: "checkmark.circle").font(.caption) }
        }
    }

    private func reload() {
        do {
            let json = try model.core.livingSpaceCommand(spaceId: spaceId, commandJson: "{\"action\":\"home\"}")
            home = try LivingSpaceProjection.decode(json)
            failure = nil
        } catch { failure = error.localizedDescription }
    }

    private func command(_ value: [String: Any]) {
        do {
            let data = try JSONSerialization.data(withJSONObject: value)
            _ = try model.core.livingSpaceCommand(spaceId: spaceId, commandJson: String(decoding: data, as: UTF8.self))
            model.refresh()
            reload()
            withAnimation(reduceMotion ? nil : .easeOut(duration: 0.15)) { saved = true }
        } catch { saved = false; failure = (error as? CoreError)?.message ?? error.localizedDescription }
    }
}
