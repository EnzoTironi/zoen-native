import AppIntents
import Foundation
internal import ExpoAppIntents
private enum ZoenNavigationError: Error { case unavailable }
struct ZoenItemEntity: AppEntity {
  static let typeDisplayRepresentation: TypeDisplayRepresentation = "Miniapp do Zoen"
  static let defaultQuery = ZoenItemQuery()
  var id: String
  @Property(title: "Nome") var name: String
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
  static let title: LocalizedStringResource = "Abrir miniapp"
  static let openAppWhenRun = true
  static let authenticationPolicy: IntentAuthenticationPolicy = .requiresLocalDeviceAuthentication
  @Parameter(title: "Miniapp") var selection: ZoenItemEntity
  static var parameterSummary: some ParameterSummary { Summary("Abrir \(\.$selection)") }
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
    return .result(dialog: "Abrindo o Zoen.")
  }
}
struct ZoenChatEntity: AppEntity {
  static let typeDisplayRepresentation: TypeDisplayRepresentation = "Conversa do Zoen"
  static let defaultQuery = ZoenChatQuery()
  var id: String
  @Property(title: "Nome") var name: String
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
  static let title: LocalizedStringResource = "Abrir conversa"
  static let openAppWhenRun = true
  static let authenticationPolicy: IntentAuthenticationPolicy = .requiresLocalDeviceAuthentication
  @Parameter(title: "Conversa") var selection: ZoenChatEntity
  static var parameterSummary: some ParameterSummary { Summary("Abrir \(\.$selection)") }
  @MainActor func perform() async throws -> some IntentResult & ProvidesDialog {
    // Saved shortcuts re-resolve IDs against the currently selected local catalog.
    guard let record = try await AppIntentEntityStore.shared.entities(ofKind: "zoen.chat", matching: [selection.id]).first,
      let identity = record.metadata["identityId"], let device = record.metadata["deviceId"] else {
      throw ZoenNavigationError.unavailable
    }
    await AppIntentDispatcher.shared.dispatch(name: "zoen.openChat", params: [
      "identityId": .string(identity), "deviceId": .string(device), "spaceId": .string(record.id)
    ])
    return .result(dialog: "Abrindo o Zoen.")
  }
}
struct ZoenShortcuts: AppShortcutsProvider {
  static var appShortcuts: [AppShortcut] {
    AppShortcut(intent: ZoenOpenItemIntent(), phrases: ["Abrir miniapp no \(.applicationName)"], shortTitle: "Abrir miniapp", systemImageName: "square.grid.2x2")
    AppShortcut(intent: ZoenOpenChatIntent(), phrases: ["Abrir conversa no \(.applicationName)"], shortTitle: "Abrir conversa", systemImageName: "bubble.left.and.bubble.right")
  }
}
