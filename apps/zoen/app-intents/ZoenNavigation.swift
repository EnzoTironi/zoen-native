import AppIntents
import Foundation
internal import ExpoAppIntents
private enum ZoenNavigationError: Error { case unavailable }
struct ZoenItemEntity: AppEntity {
  static let typeDisplayRepresentation: TypeDisplayRepresentation = "Zoen app"
  static let defaultQuery = ZoenItemQuery()
  var id: String
  @Property(title: "Name") var name: String
  var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }
  init(record: AppIntentEntityRecord) { id = record.id; name = record.title }
}
struct ZoenItemQuery: EntityStringQuery {
  func entities(for identifiers: [String]) async throws -> [ZoenItemEntity] {
    try await AppIntentEntityStore.shared.entities(ofKind: "zoen.item", matching: identifiers).map(ZoenItemEntity.init(record:))
  }
  func suggestedEntities() async throws -> [ZoenItemEntity] {
    try await AppIntentEntityStore.shared.entities(ofKind: "zoen.item").map(ZoenItemEntity.init(record:))
  }
  func entities(matching string: String) async throws -> [ZoenItemEntity] {
    try await suggestedEntities().filter { $0.name.localizedStandardContains(string) }
  }
}
struct ZoenOpenItemIntent: AppIntent {
  static let title: LocalizedStringResource = "Open app"
  static let openAppWhenRun = true
  static let authenticationPolicy: IntentAuthenticationPolicy = .requiresLocalDeviceAuthentication
  @Parameter(title: "App") var selection: ZoenItemEntity
  static var parameterSummary: some ParameterSummary { Summary("Open \(\.$selection)") }
  @MainActor func perform() async throws -> some IntentResult & ProvidesDialog {
    // Saved shortcuts re-resolve IDs against the currently selected local catalog.
    guard let record = try await AppIntentEntityStore.shared.entities(ofKind: "zoen.item", matching: [selection.id]).first,
      let identity = record.metadata["identityId"], let device = record.metadata["deviceId"] else {
      throw ZoenNavigationError.unavailable
    }
    guard let rawVersion = record.metadata["version"], let version = Int(rawVersion), version >= 0 else { throw ZoenNavigationError.unavailable }
    await AppIntentDispatcher.shared.dispatch(name: "zoen.openItem", params: [
      "identityId": .string(identity), "deviceId": .string(device), "itemId": .string(record.id), "version": .int(version)
    ])
    return .result(dialog: "Opening Zoen.")
  }
}
struct ZoenChatEntity: AppEntity {
  static let typeDisplayRepresentation: TypeDisplayRepresentation = "Zoen conversation"
  static let defaultQuery = ZoenChatQuery()
  var id: String
  @Property(title: "Name") var name: String
  var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }
  init(record: AppIntentEntityRecord) { id = record.id; name = record.title }
}
struct ZoenChatQuery: EntityStringQuery {
  func entities(for identifiers: [String]) async throws -> [ZoenChatEntity] {
    try await AppIntentEntityStore.shared.entities(ofKind: "zoen.chat", matching: identifiers).map(ZoenChatEntity.init(record:))
  }
  func suggestedEntities() async throws -> [ZoenChatEntity] {
    try await AppIntentEntityStore.shared.entities(ofKind: "zoen.chat").map(ZoenChatEntity.init(record:))
  }
  func entities(matching string: String) async throws -> [ZoenChatEntity] {
    try await suggestedEntities().filter { $0.name.localizedStandardContains(string) }
  }
}
struct ZoenOpenChatIntent: AppIntent {
  static let title: LocalizedStringResource = "Open conversation"
  static let openAppWhenRun = true
  static let authenticationPolicy: IntentAuthenticationPolicy = .requiresLocalDeviceAuthentication
  @Parameter(title: "Conversation") var selection: ZoenChatEntity
  static var parameterSummary: some ParameterSummary { Summary("Open \(\.$selection)") }
  @MainActor func perform() async throws -> some IntentResult & ProvidesDialog {
    // Saved shortcuts re-resolve IDs against the currently selected local catalog.
    guard let record = try await AppIntentEntityStore.shared.entities(ofKind: "zoen.chat", matching: [selection.id]).first,
      let identity = record.metadata["identityId"], let device = record.metadata["deviceId"] else {
      throw ZoenNavigationError.unavailable
    }
    await AppIntentDispatcher.shared.dispatch(name: "zoen.openChat", params: [
      "identityId": .string(identity), "deviceId": .string(device), "spaceId": .string(record.id)
    ])
    return .result(dialog: "Opening Zoen.")
  }
}
struct ZoenShortcuts: AppShortcutsProvider {
  static var appShortcuts: [AppShortcut] {
    AppShortcut(intent: ZoenOpenItemIntent(), phrases: ["Open an app in \(.applicationName)"], shortTitle: "Open app", systemImageName: "square.grid.2x2")
    AppShortcut(intent: ZoenOpenChatIntent(), phrases: ["Open a conversation in \(.applicationName)"], shortTitle: "Open conversation", systemImageName: "bubble.left.and.bubble.right")
  }
}
