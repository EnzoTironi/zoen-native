import CoreGraphics
import CryptoKit
import Darwin
import Foundation
import PDFKit

// Run seed/recover as separate processes against a NEW synthetic /private/tmp
// root. The only replacement is presentation; the journal, Swift typed bridge,
// UniFFI, encryption, SQLite, and canonical file mutations are production code.
enum CorePDFFailure: Error { case assertion(String), unsafeRoot }
let corePDFMaximum: UInt64 = 64 << 20
let corePDFMarker = "Zoen.PDFEditCoreTests.v1"
let corePDFAnnotations = ["Synthetic underline A", "Synthetic underline B"]

func corePDFCheck(_ condition: Bool, _ message: String) throws {
  if !condition { throw CorePDFFailure.assertion(message) }
}

func corePDFHash(_ data: Data) -> String {
  SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

func corePDFRegular(_ url: URL, maximum: UInt64) throws -> Data {
  let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
  try corePDFCheck(attributes[.type] as? FileAttributeType == .typeRegular,
                   "Expected an owned regular fixture file")
  try corePDFCheck((attributes[.ownerAccountID] as? NSNumber)?.uint32Value == getuid(),
                   "Fixture file belongs to another user")
  let size = (attributes[.size] as? NSNumber)?.uint64Value ?? UInt64.max
  try corePDFCheck(size <= maximum, "Fixture file exceeds its bound")
  return try Data(contentsOf: url)
}

func corePDFWrite(_ bytes: Data, to url: URL) throws {
  let manager = FileManager.default
  let temporary = url.deletingLastPathComponent().appendingPathComponent(".write-\(UUID().uuidString)")
  guard manager.createFile(atPath: temporary.path, contents: nil,
                           attributes: [.posixPermissions: 0o600]) else {
    throw CorePDFFailure.assertion("Could not create private fixture file")
  }
  defer { try? manager.removeItem(at: temporary) }
  let handle = try FileHandle(forWritingTo: temporary)
  do { try handle.write(contentsOf: bytes); try handle.synchronize(); try handle.close() }
  catch { try? handle.close(); throw error }
  // POSIX rename replaces one exact owned file without making secrets readable
  // between creation and chmod. No key value is ever encoded in test output.
  guard Darwin.rename(temporary.path, url.path) == 0 else {
    throw CorePDFFailure.assertion("Could not persist private fixture file")
  }
}

final class CorePDFDiskVault: SecretVault, @unchecked Sendable {
  private let root: URL
  private let lock = NSLock()
  init(root: URL) throws {
    self.root = root
    let attributes = try FileManager.default.attributesOfItem(atPath: root.path)
    try corePDFCheck(attributes[.type] as? FileAttributeType == .typeDirectory,
                     "Vault must be an owned directory")
    try corePDFCheck((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700,
                     "Vault permissions must be 0700")
  }
  private func location(_ key: String) -> URL? {
    guard !key.isEmpty, key.utf8.count <= 512 else { return nil }
    return root.appendingPathComponent(corePDFHash(Data(key.utf8)))
  }
  func load(key: String) -> Data? {
    lock.lock(); defer { lock.unlock() }
    guard let url = location(key),
          let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
          (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600 else { return nil }
    return try? corePDFRegular(url, maximum: 64 << 10)
  }
  func save(key: String, value: Data) -> Bool {
    lock.lock(); defer { lock.unlock() }
    guard let url = location(key), value.count <= 64 << 10 else { return false }
    do { try corePDFWrite(value, to: url); return true } catch { return false }
  }
  func delete(key: String) {
    lock.lock(); defer { lock.unlock() }
    if let url = location(key) { try? FileManager.default.removeItem(at: url) }
  }
}

// This document contains public identifiers and hashes only, never draft bytes,
// vault contents, private keys, or a portable account backup.
struct CorePDFState: Codable {
  let format: String
  let conflict: Bool
  let identityID: String
  let deviceID: String
  let spaceID: String
  let itemID: String
  let originVersion: UInt32
  let originHash: String
  let editorID: String
  let revision: String
  let captureIDs: [String]
  let hashes: [String]
  let sizes: [UInt64]
  let canonicalVersion: UInt32
  let canonicalHash: String
  let events: UInt64
  let headHash: String
  let pendingEvents: UInt64

  var scope: FileEditDraftScope {
    .init(expectedIdentityId: identityID, expectedDeviceId: deviceID, spaceId: spaceID, itemId: itemID)
  }
  var origin: ZoenFilePreview.EditBase { .init(version: originVersion, sha256: originHash) }
}

struct CorePDFFixture {
  let root: URL
  var database: URL { root.appendingPathComponent("device.sqlite") }
  var stateURL: URL { root.appendingPathComponent("public-state.json") }
  var leases: URL { root.appendingPathComponent("leases", isDirectory: true) }
  var vaultURL: URL { root.appendingPathComponent("vault", isDirectory: true) }

  init(path: String, create: Bool) throws {
    // Check the raw, single-component name before constructing a URL. URL
    // equality includes Foundation's directory/normalization representation;
    // it is not a filesystem ancestry check.
    let prefix = "/private/tmp/"
    guard path.hasPrefix(prefix) else { throw CorePDFFailure.unsafeRoot }
    let name = String(path.dropFirst(prefix.count))
    guard name.hasPrefix("zoen-pdf-edit-core-"), name.utf8.count <= 120,
          name.utf8.allSatisfy({
            (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45
          }) else { throw CorePDFFailure.unsafeRoot }
    let temporary = URL(fileURLWithPath: "/private/tmp", isDirectory: true).resolvingSymlinksInPath()
    root = temporary.appendingPathComponent(name, isDirectory: true)
    let manager = FileManager.default
    if create {
      guard !manager.fileExists(atPath: root.path) else { throw CorePDFFailure.unsafeRoot }
      try manager.createDirectory(at: root, withIntermediateDirectories: false,
                                  attributes: [.posixPermissions: 0o700])
      try manager.createDirectory(at: vaultURL, withIntermediateDirectories: false,
                                  attributes: [.posixPermissions: 0o700])
    }
    let attributes = try manager.attributesOfItem(atPath: root.path)
    guard attributes[.type] as? FileAttributeType == .typeDirectory,
          (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == getuid(),
          (attributes[.posixPermissions] as? NSNumber)?.intValue == 0o700 else {
      throw CorePDFFailure.unsafeRoot
    }
  }

  func lease(_ data: Data) throws -> ZoenPreviewFile {
    let manager = FileManager.default
    try manager.createDirectory(at: leases, withIntermediateDirectories: true,
                                attributes: [.posixPermissions: 0o700])
    let directory = leases.appendingPathComponent(UUID().uuidString, isDirectory: true)
    try manager.createDirectory(at: directory, withIntermediateDirectories: false,
                                attributes: [.posixPermissions: 0o700])
    let file = ZoenPreviewFile(url: directory.appendingPathComponent("synthetic.pdf"), directory: directory)
    do { try corePDFWrite(data, to: file.url) }
    catch { try? file.remove(); throw error }
    return file
  }

  func finishLeases() throws {
    let manager = FileManager.default
    if manager.fileExists(atPath: leases.path) {
      try corePDFCheck(try manager.contentsOfDirectory(atPath: leases.path).isEmpty,
                       "A plaintext PDF lease survived phase completion")
      try manager.removeItem(at: leases)
    }
    let names = try manager.contentsOfDirectory(atPath: root.path)
    try corePDFCheck(names.allSatisfy {
      ["vault", "public-state.json", "device.sqlite", "device.sqlite-wal", "device.sqlite-shm"].contains($0)
    }, "Unexpected plaintext fixture output")
  }
}

func corePDFOriginal() throws -> Data {
  let output = NSMutableData()
  var box = CGRect(x: 0, y: 0, width: 300, height: 200)
  guard let consumer = CGDataConsumer(data: output as CFMutableData),
        let context = CGContext(consumer: consumer, mediaBox: &box,
          [kCGPDFContextTitle as String: "Synthetic PDF edit core fixture"] as CFDictionary) else {
    throw CorePDFFailure.assertion("Could not make synthetic PDF")
  }
  context.beginPDFPage(nil)
  context.setFillColor(gray: 0.2, alpha: 1)
  context.fill(CGRect(x: 30, y: 110, width: 210, height: 2))
  context.fill(CGRect(x: 30, y: 70, width: 210, height: 2))
  context.endPDFPage(); context.closePDF()
  return Data(bytes: output.bytes, count: output.length)
}

func corePDFAnnotate(_ original: Data, contents: [String]) throws -> Data {
  guard let document = PDFDocument(data: original), let page = document.page(at: 0),
        document.pageCount == 1, !document.isLocked else {
    throw CorePDFFailure.assertion("Synthetic PDF is invalid")
  }
  for (index, content) in contents.enumerated() {
    let annotation = PDFAnnotation(bounds: CGRect(x: 30, y: CGFloat(104 - index * 40), width: 210, height: 12),
                                   forType: .underline, withProperties: nil)
    annotation.contents = content
    page.addAnnotation(annotation)
  }
  guard let result = document.dataRepresentation() else {
    throw CorePDFFailure.assertion("Could not encode synthetic annotations")
  }
  return result
}

func corePDFValidate(_ bytes: Data, annotations: [String]) throws {
  guard let document = PDFDocument(data: bytes), document.pageCount == 1,
        !document.isLocked, let page = document.page(at: 0) else {
    throw CorePDFFailure.assertion("Recovered PDF is not a readable one-page document")
  }
  try corePDFCheck(page.annotations.map(\.contents) == annotations.map(Optional.some),
                   "Recovered annotation contents/order changed")
  let underlineType = PDFAnnotation(bounds: .zero, forType: .underline, withProperties: nil).type
  try corePDFCheck(page.annotations.allSatisfy { $0.type == underlineType },
                   "Recovered annotation subtype changed")
}

@MainActor final class CorePDFSignal {
  private var count = 0
  private var waiting: [(Int, CheckedContinuation<Void, Never>)] = []
  func emit() {
    count += 1
    let ready = waiting.filter { $0.0 <= count }
    waiting.removeAll { $0.0 <= count }
    ready.forEach { $0.1.resume() }
  }
  func wait(_ target: Int = 1) async {
    if count < target { await withCheckedContinuation { waiting.append((target, $0)) } }
  }
}

@MainActor final class CorePDFPresentation: ZoenFilePreviewPresentation {
  let shown = CorePDFSignal()
  let recovery = CorePDFSignal()
  var ended: (@MainActor (Error?) -> Void)?
  var retry: (@MainActor () -> Void)?
  var keep: (@MainActor () -> Void)?
  var discard: (@MainActor () -> Void)?
  var error: Error?
  var changesSaved = false
  var shownURL: URL?
  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    shownURL = file; self.ended = ended; shown.emit()
  }
  func dismiss(completion: @escaping @MainActor () -> Void) { completion() }
  func recover(error: Error, changesSaved: Bool, retry: (@MainActor () -> Void)?,
               keep: (@MainActor () -> Void)?, discard: @escaping @MainActor () -> Void) {
    self.error = error; self.changesSaved = changesSaved
    self.retry = retry; self.keep = keep; self.discard = discard
    recovery.emit()
  }
}
