import Foundation

/// Quick Look's edited-copy URL is temporary. Acquire our own copy before the
/// delegate returns; no later actor hop depends on the framework's file lease.
enum ZoenPDFEditCapture {
  struct Failure: Error {
    let cause: Error
    let retained: ZoenPreviewFile?
  }

  static func stage(_ source: URL, maximumBytes: UInt64) -> Result<ZoenPreviewFile, Failure> {
    let manager = FileManager.default
    var copy: ZoenPreviewFile?
    let scoped = source.startAccessingSecurityScopedResource()
    defer { if scoped { source.stopAccessingSecurityScopedResource() } }
    do {
      try validate(source, maximumBytes: maximumBytes)
      let directory = URL.cachesDirectory.appendingPathComponent("ZoenMedia/\(UUID().uuidString)", isDirectory: true)
      try manager.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
      let file = ZoenPreviewFile(url: directory.appendingPathComponent("edited.pdf"), directory: directory)
      copy = file
      try manager.copyItem(at: source, to: file.url)
      try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.url.path)
      #if os(iOS)
      try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: directory.path)
      try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: file.url.path)
      #endif
      try validate(file.url, maximumBytes: maximumBytes)
      return .success(file)
    } catch {
      if let copy {
        do { try copy.remove() }
        catch { return .failure(Failure(cause: error, retained: copy)) }
      }
      return .failure(Failure(cause: error, retained: nil))
    }
  }

  private static func validate(_ url: URL, maximumBytes: UInt64) throws {
    guard url.isFileURL else { throw ZoenFilePreview.Failure.editedPDF }
    let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
    guard attributes[.type] as? FileAttributeType == .typeRegular,
          let size = attributes[.size] as? NSNumber,
          size.uint64Value >= 5, size.uint64Value <= maximumBytes else { throw ZoenFilePreview.Failure.editedPDF }
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    guard try handle.read(upToCount: 5) == Data("%PDF-".utf8) else { throw ZoenFilePreview.Failure.editedPDF }
  }
}
