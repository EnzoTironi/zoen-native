import Foundation
import ImageIO
import UniformTypeIdentifiers
import Darwin

private struct AttachmentJourneyFailure: Error, CustomStringConvertible {
    let description: String
}

private actor ImportGate {
    private var opened = false
    private var continuation: CheckedContinuation<Void, Never>?
    func wait() async {
        if opened { return }
        await withCheckedContinuation { continuation = $0 }
    }
    func open() { opened = true; continuation?.resume(); continuation = nil }
}

@main
struct AttachmentFilesJourney {
    static func note(_ message: String) { FileHandle.standardOutput.write(Data((message + "\n").utf8)) }

    static func check(_ condition: Bool, _ description: String) throws {
        guard condition else { throw AttachmentJourneyFailure(description: description) }
    }

    static func rejects(_ failure: AttachmentFiles.Failure, _ action: () throws -> Void) throws {
        do { try action() }
        catch let actual as AttachmentFiles.Failure {
            try check(String(describing: actual) == String(describing: failure), "Wrong rejection: \(actual)")
            return
        }
        throw AttachmentJourneyFailure(description: "Selection was not rejected: \(failure)")
    }

    static func children(_ root: URL) throws -> [URL] {
        guard FileManager.default.fileExists(atPath: root.path) else { return [] }
        return try FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)
    }

    static func line(from handle: FileHandle) throws -> Data {
        var contents = Data()
        for _ in 0..<4096 {
            var event = pollfd(fd: handle.fileDescriptor, events: Int16(POLLIN), revents: 0)
            guard Darwin.poll(&event, 1, 5_000) > 0 else {
                throw AttachmentJourneyFailure(description: "The owned subprocess did not publish its session in time")
            }
            var byte: UInt8 = 0
            guard Darwin.read(handle.fileDescriptor, &byte, 1) == 1 else {
                throw AttachmentJourneyFailure(description: "The owned subprocess closed before publishing its session")
            }
            if byte == 10 { return contents }
            contents.append(byte)
        }
        throw AttachmentJourneyFailure(description: "The owned subprocess returned an oversized session path")
    }

    static func sparseFile(_ url: URL, size: UInt64) throws {
        guard FileManager.default.createFile(atPath: url.path, contents: nil) else { throw CocoaError(.fileWriteUnknown) }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.truncate(atOffset: size)
    }

    static func jpeg(width: Int, height: Int) throws -> Data {
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
            bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue), let image = context.makeImage() else {
            throw CocoaError(.coderInvalidValue)
        }
        let bytes = NSMutableData()
        guard let output = CGImageDestinationCreateWithData(bytes, UTType.jpeg.identifier as CFString, 1, nil) else {
            throw CocoaError(.coderInvalidValue)
        }
        CGImageDestinationAddImage(output, image, nil)
        guard CGImageDestinationFinalize(output) else { throw CocoaError(.coderInvalidValue) }
        return bytes as Data
    }

    static func largeCompressedTIFF() -> Data {
        // Valid PackBits data: 67.1 million monochrome pixels fit in a 1 MB file.
        let compressedCount = 8192 * 8192 / 128 * 2
        var pixels = Data(count: compressedCount)
        for index in stride(from: 0, to: compressedCount, by: 2) { pixels[index] = 129 }
        var data = Data([0x49, 0x49, 0x2a, 0, 8, 0, 0, 0])
        func append16(_ value: UInt16) {
            var bytes = value.littleEndian
            withUnsafeBytes(of: &bytes) { data.append(contentsOf: $0) }
        }
        func append32(_ value: UInt32) {
            var bytes = value.littleEndian
            withUnsafeBytes(of: &bytes) { data.append(contentsOf: $0) }
        }
        let tags: [(UInt16, UInt16, UInt32)] = [(256, 4, 8192), (257, 4, 8192),
            (258, 3, 8), (259, 3, 32773), (262, 3, 1), (273, 4, 122),
            (277, 3, 1), (278, 4, 8192), (279, 4, UInt32(compressedCount))]
        append16(UInt16(tags.count))
        for (tag, type, value) in tags { append16(tag); append16(type); append32(1); append32(value) }
        append32(0)
        data.append(pixels)
        return data
    }

    static func main() async throws {
        if CommandLine.arguments.count == 3, CommandLine.arguments[1] == "--hold-staging-session" {
            let session = try AttachmentStagingSession(root: URL(fileURLWithPath: CommandLine.arguments[2]))
            try Data("Owned temporary copy before crash".utf8).write(to: session.directory.appendingPathComponent("review.txt"))
            note(session.directory.path)
            _ = try FileHandle.standardInput.read(upToCount: 1)
            withExtendedLifetime(session) {}
            return
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-attachment-journey-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let originals = root.appendingPathComponent("originals")
        try FileManager.default.createDirectory(at: originals, withIntermediateDirectories: true)
        let staged = root.appendingPathComponent("staged")
        let source = originals.appendingPathComponent("notes.txt")
        let original = Data("A file reviewed before sending. Olá 雨".utf8)
        try original.write(to: source)

        var owner: AttachmentFile? = try AttachmentFiles.stage([source], root: staged)[0]
        let url = owner!.url
        try Data("The external provider changed its file".utf8).write(to: source)
        try check(try AttachmentFiles.read(url) == original, "Review copy changed with the external file")
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        try check((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600, "Plaintext copy is not private")
        try check(try url.deletingLastPathComponent().resourceValues(forKeys: [.isExcludedFromBackupKey]).isExcludedFromBackup == true,
                  "Plaintext review copy can enter backups")
        var previewOwner = owner
        owner = nil
        try check(FileManager.default.fileExists(atPath: url.path), "Preview lost its file while still open")
        try check(previewOwner?.url == url, "Preview owner changed")
        previewOwner = nil
        try check(!FileManager.default.fileExists(atPath: url.path), "Closing the last owner kept plaintext")
        note("PASS external provider isolation, private storage and preview lifetime")

        let safe = try AttachmentFiles.stage(original, name: "../../folder\\secret\u{0000}.txt", root: staged)
        try check(safe.name == "secret.txt" && safe.url.deletingLastPathComponent().deletingLastPathComponent().standardizedFileURL.path == staged.standardizedFileURL.path,
                  "A supplied name escaped its own directory")
        let empty = originals.appendingPathComponent("empty.txt")
        try Data().write(to: empty)
        let failed = root.appendingPathComponent("failed")
        try rejects(.empty) { _ = try AttachmentFiles.stage([source, empty], root: failed) }
        try check(try children(failed).isEmpty, "A failed selection left a partial plaintext copy")
        try rejects(.notAFile) { _ = try AttachmentFiles.stage([originals], root: failed) }
        let fifo = originals.appendingPathComponent("pipe")
        guard mkfifo(fifo.path, 0o600) == 0 else { throw CocoaError(.fileWriteUnknown) }
        try rejects(.notAFile) { _ = try AttachmentFiles.stage([fifo], root: failed) }
        try rejects(.notAFile) { _ = try AttachmentFiles.read(fifo) }
        let symlink = originals.appendingPathComponent("linked.txt")
        try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: source)
        try rejects(.notAFile) { _ = try AttachmentFiles.stage([symlink], root: failed) }
        try rejects(.tooMany) { _ = try AttachmentFiles.stage(Array(repeating: source, count: 11), root: failed) }
        note("PASS supplied paths, nonblocking FIFO/symlink rejection and partial cleanup")

        let large = originals.appendingPathComponent("large.bin")
        try sparseFile(large, size: UInt64(AttachmentFiles.maximumBytes + 1))
        try rejects(.tooLarge) { _ = try AttachmentFiles.stage([large], root: failed) }
        let full = originals.appendingPathComponent("full.bin")
        try sparseFile(full, size: UInt64(AttachmentFiles.maximumBytes))
        try rejects(.selectionTooLarge) { _ = try AttachmentFiles.stage([full, full, source], root: failed) }
        try check(try children(failed).isEmpty, "Size rejection kept earlier files in the selection")
        note("PASS per-file and aggregate size limits before sending")

        let photo = try AttachmentFiles.stage(jpeg(width: 800, height: 600), name: "photo.jpg", root: staged)
        let thumbnail = try AttachmentFiles.imageThumbnail(photo.url)
        guard let thumbnail, let imageSource = CGImageSourceCreateWithData(thumbnail as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(imageSource, 0, nil) else {
            throw AttachmentJourneyFailure(description: "Photo review has no real thumbnail")
        }
        try check(image.width <= 480 && image.height <= 480, "Review decoded a full-size photo")
        try rejects(.imageTooLarge) { _ = try AttachmentFiles.stage(largeCompressedTIFF(), name: "oversized.tiff", root: failed) }
        try rejects(.invalidImage) { _ = try AttachmentFiles.stage(original, name: "broken.jpg", root: failed) }
        try check(try children(failed).isEmpty, "Image rejection left decrypted copies")
        note("PASS bounded real photo preview and hostile image metadata")

        let cancelled = root.appendingPathComponent("cancelled")
        let barrier = ImportGate()
        let work = Task.detached {
            await barrier.wait()
            return try AttachmentFiles.stage([source], root: cancelled)
        }
        work.cancel()
        await barrier.open()
        do {
            _ = try await work.value
            throw AttachmentJourneyFailure(description: "Cancelled import created a review copy")
        } catch is CancellationError {}
        try check(try children(cancelled).isEmpty, "Cancelled import retained plaintext")
        note("PASS cancellation before import without retained plaintext")

        let reviewed = try AttachmentFiles.stage(original, name: "retained-review.txt", root: staged)
        let removed = try AttachmentFiles.stage(original, name: "removed-selection.txt", root: staged)
        let other = try AttachmentFiles.stage(original, name: "other-preview.txt", root: staged)
        let draft = AttachmentDraft(files: [reviewed, removed])
        let retainedSheet = { draft.files }
        guard let firstPreview = reviewed.beginUse(), let secondPreview = reviewed.beginUse() else {
            throw AttachmentJourneyFailure(description: "The review could not open its preview leases")
        }
        draft.discard()
        try check(try AttachmentFiles.read(reviewed.url) == original, "Closing review removed a live preview's bytes")
        try check(!FileManager.default.fileExists(atPath: removed.url.path), "A retained sheet closure kept a closed selection on disk")
        try check(reviewed.beginUse() == nil, "A closed selection started a new preview")
        firstPreview.close()
        try check(FileManager.default.fileExists(atPath: reviewed.url.path), "One preview's close removed another preview's file")
        secondPreview.close()
        secondPreview.close()
        draft.discard()
        try check(!FileManager.default.fileExists(atPath: reviewed.url.path), "The last preview's close retained a dismissed review copy")
        try check(try AttachmentFiles.read(other.url) == original, "Dismissal cleanup removed another attachment's preview")
        try check(retainedSheet().map(\.url) == [reviewed.url, removed.url], "The retained closure no longer models SwiftUI's references")
        let sent = try AttachmentFiles.stage(original, name: "sent.txt", root: staged)
        let retainedSuccess = { sent.url }
        sent.discard()
        try check(!FileManager.default.fileExists(atPath: retainedSuccess().path), "A retained successful-send closure kept plaintext")
        withExtendedLifetime((retainedSheet, retainedSuccess, other)) {}
        note("PASS dismissal/send cleanup despite retained closures, with independent preview leases")

        let sessions = root.appendingPathComponent("sessions")
        let active = try AttachmentStagingSession(root: sessions)
        let activeCopy = active.directory.appendingPathComponent("active.txt")
        try original.write(to: activeCopy)
        let unknown = sessions.appendingPathComponent("unrelated")
        try FileManager.default.createDirectory(at: unknown, withIntermediateDirectories: true)
        let unrelatedCopy = unknown.appendingPathComponent("keep.txt")
        try original.write(to: unrelatedCopy)
        let child = Process()
        child.executableURL = URL(fileURLWithPath: CommandLine.arguments[0])
        child.arguments = ["--hold-staging-session", sessions.path]
        let output = Pipe()
        let input = Pipe()
        child.standardOutput = output
        child.standardInput = input
        child.standardError = FileHandle.standardError
        try child.run()
        defer {
            if child.isRunning { child.terminate(); child.waitUntilExit() }
            try? input.fileHandleForWriting.close()
        }
        try? output.fileHandleForWriting.close()
        let pathBytes = try line(from: output.fileHandleForReading)
        let childPath = String(decoding: pathBytes, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        try check(childPath.hasPrefix(sessions.path + "/session-"), "The owned subprocess did not publish a staging session")
        let childDirectory = URL(fileURLWithPath: childPath)
        let another = try AttachmentStagingSession(root: sessions)
        try check(FileManager.default.fileExists(atPath: childDirectory.path), "Sweep removed another live process's staging session")
        try check(try Data(contentsOf: activeCopy) == original, "Sweep removed a current preview")
        // Only this test's own child is killed. No application services are touched.
        try check(Darwin.kill(child.processIdentifier, SIGKILL) == 0, "Could not simulate the owned staging process crashing")
        child.waitUntilExit()
        let reopened = try AttachmentStagingSession(root: sessions)
        try check(!FileManager.default.fileExists(atPath: childDirectory.path), "Relaunch kept the crashed process's plaintext copy")
        try check(try Data(contentsOf: activeCopy) == original && Data(contentsOf: unrelatedCopy) == original,
                  "Crash cleanup touched an active session or unrelated directory")
        withExtendedLifetime((active, another, reopened)) {}
        note("PASS abandoned crash cleanup with live-process and unrelated-directory preservation")
        note("RESULT 7 attachment journeys passed; 0 failed")
    }
}
