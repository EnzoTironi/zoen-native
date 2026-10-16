import Foundation
import CryptoKit
import ZoenCore

// The vault is the only fake. Checkpoints, crypto, DTOs, and the reopen journey use production code.
private final class MemoryDraftVault: SecretVault, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [String: Data] = [:]
    private var writes = 0
    private var unavailableAfterCheckpoint: URL?

    func load(key: String) -> Data? {
        lock.withLock {
            if let root = unavailableAfterCheckpoint,
               let entries = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil),
               entries.contains(where: { ($0 as? URL)?.pathExtension == "draft" }) {
                values.removeValue(forKey: key)
            }
            return values[key]
        }
    }
    func save(key: String, value: Data) -> Bool {
        lock.withLock { values[key] = value; writes += 1 }
        return true
    }
    func delete(key: String) { _ = lock.withLock { values.removeValue(forKey: key) } }
    var snapshot: [String: Data] { lock.withLock { values } }
    var saveCount: Int { lock.withLock { writes } }
    func replace(_ snapshot: [String: Data]) { lock.withLock { values = snapshot } }
    func loseKeyAfterCheckpoint(under root: URL?) { lock.withLock { unavailableAfterCheckpoint = root } }
}

private enum JourneyError: Error { case failed(String) }

@main @MainActor
struct PageDraftStoreJourney {
    static func check(_ condition: Bool, _ message: String) throws {
        if !condition { throw JourneyError.failed(message) }
    }

    static func refused(_ message: String, _ action: () throws -> Void) throws {
        do { try action() } catch { return }
        throw JourneyError.failed(message)
    }

    static func block(_ text: String) -> PageBlockDto {
        PageBlockDto(id: "paragraph", kind: "paragraph", level: 0, indent: 0, number: 0,
                     checked: false, lang: "", url: "", alt: "", text: text,
                     spans: [TextSpanDto(start: 0, end: 5, key: "bold", value: "true")])
    }

    static func draft(_ text: String, mutation: String = UUID().uuidString) -> PageEditorDraft {
        PageEditorDraft(mutationId: mutation, editContext: "observed-context",
                        blocks: [block(text)], baseline: [block("Original")])
    }

    static func files(_ root: URL) throws -> [URL] {
        guard FileManager.default.fileExists(atPath: root.path) else { return [] }
        let children = try FileManager.default.contentsOfDirectory(at: root,
            includingPropertiesForKeys: [.isDirectoryKey])
        return try children.flatMap { url -> [URL] in
            if try url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true { return try files(url) }
            return url.pathExtension == "draft" ? [url] : []
        }
    }

    private static func run(_ name: String, _ journey: (URL, String, MemoryDraftVault) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("zoen-page-draft-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        FileHandle.standardOutput.write(Data("START \(name)\n".utf8))
        try journey(root, UUID().uuidString, MemoryDraftVault())
        FileHandle.standardOutput.write(Data("PASS \(name)\n".utf8))
    }

    static func main() throws {
        try run("independent windows preserve their own latest encrypted edit") { root, actor, vault in
            let a = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            let b = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { a.release(); b.release() }
            try a.save(draft("First keystroke"))
            let latest = draft("Olá 👩🏽‍💻 雨", mutation: "latest-edit")
            try a.save(latest)
            try b.save(draft("Other window"))
            try check(try b.recoverable().isEmpty, "An active editor was stolen by another window")
            try check(try files(root).count == 2, "Windows overwrote each other's checkpoint")
            a.release()
            let recovered = try b.recoverable()
            try check(recovered.count == 1 && recovered[0].draft.mutationId == latest.mutationId,
                      "The latest mutation was not retained")
            try check(recovered[0].draft.blocks == latest.blocks && recovered[0].draft.base == latest.base &&
                      recovered[0].draft.editContext == latest.editContext, "Content/context changed during recovery")
            let cipher = try Data(contentsOf: recovered[0].url)
            try check(cipher.range(of: Data("Olá".utf8)) == nil, "Draft was stored in plaintext")
            try b.acknowledge(recovered[0])
            try check(try files(root).count == 1, "Acknowledgement removed another window's draft")
        }

        try run("missing live vault key cannot be replaced between windows") { root, actor, vault in
            let a = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { a.release() }
            let keys = vault.snapshot
            let writes = vault.saveCount
            vault.replace([:])
            try refused("Opening a second window replaced a live key") {
                let b = try PageDraftStore(actor: actor, space: "chat", item: "other", vault: vault, root: root)
                b.release()
            }
            try check(vault.saveCount == writes, "A new key was written after live key loss")
            try refused("Save succeeded without a durable key") { try a.save(draft("Unfinished IME 雨")) }
            try check(try files(root).isEmpty, "Save wrote unrecoverable ciphertext")
            vault.replace(keys)
            try a.save(draft("Recovered key 雨"))
            a.release()
            let restarted = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { restarted.release() }
            try check(try restarted.recoverable().first?.draft.blocks[0].text == "Recovered key 雨",
                      "Restoring the durable key did not recover the edit")
        }

        try run("superseded key refuses save and preserves previous ciphertext") { root, actor, vault in
            let a = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { a.release() }
            try a.save(draft("Keep this edit"))
            let url = try files(root)[0]
            let cipher = try Data(contentsOf: url)
            let original = vault.snapshot
            let keyName = try original.keys.first.unwrap("Missing generated key")
            vault.replace([keyName: Data(repeating: 5, count: 32)])
            try refused("Save accepted a superseded key") { try a.save(draft("Do not replace the old edit")) }
            try check(try Data(contentsOf: url) == cipher, "Key failure overwrote previous ciphertext")
            vault.replace(original)
            a.release()
            let restarted = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { restarted.release() }
            try check(try restarted.recoverable()[0].draft.blocks[0].text == "Keep this edit", "Previous draft was lost")
        }

        try run("key loss after the atomic write reports failure and retains recovery bytes") { root, actor, vault in
            let editor = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { editor.release() }
            let original = vault.snapshot
            vault.loseKeyAfterCheckpoint(under: root)
            try refused("Save reported success after losing its durable key") { try editor.save(draft("Keep even an uncertain write")) }
            let checkpoints = try files(root)
            try check(checkpoints.count == 1, "Uncertain save deleted the checkpoint")
            let cipher = try Data(contentsOf: checkpoints[0])
            vault.loseKeyAfterCheckpoint(under: nil)
            vault.replace(original)
            editor.release()
            let restarted = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            defer { restarted.release() }
            try check(try restarted.recoverable()[0].draft.blocks[0].text == "Keep even an uncertain write",
                      "Key restoration could not recover an uncertain write")
            try check(try Data(contentsOf: checkpoints[0]) == cipher, "Recovery changed the original ciphertext")
        }

        try run("unreadable actor subtree does not authorize a replacement key") { root, actor, vault in
            let a = try PageDraftStore(actor: actor, space: "chat", item: "old-page", vault: vault, root: root)
            try a.save(draft("Protected edit")); a.release()
            let url = try files(root)[0]
            let cipher = try Data(contentsOf: url)
            let directory = url.deletingLastPathComponent()
            let original = vault.snapshot
            let writes = vault.saveCount
            try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: directory.path)
            defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path) }
            vault.replace([:])
            try refused("An inaccessible subtree was treated as empty") {
                let b = try PageDraftStore(actor: actor, space: "chat", item: "new-page", vault: vault, root: root)
                b.release()
            }
            try check(vault.saveCount == writes, "Unreadable checkpoint led to a replacement vault key")
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
            try check(try Data(contentsOf: url) == cipher, "Protected checkpoint changed")
            vault.replace(original)
            let restarted = try PageDraftStore(actor: actor, space: "chat", item: "old-page", vault: vault, root: root)
            defer { restarted.release() }
            try check(try restarted.recoverable()[0].draft.blocks[0].text == "Protected edit", "Protected edit lost")
        }

        try run("ciphertext cannot be moved to another page or silently repaired") { root, actor, vault in
            let a = try PageDraftStore(actor: actor, space: "chat", item: "page-a", vault: vault, root: root)
            let b = try PageDraftStore(actor: actor, space: "chat", item: "page-b", vault: vault, root: root)
            try a.save(draft("Page A")); try b.save(draft("Page B"))
            a.release(); b.release()
            let ra = try a.recoverable()[0]
            let rb = try b.recoverable()[0]
            let wrongScope = try Data(contentsOf: ra.url)
            try wrongScope.write(to: rb.url, options: .atomic)
            try refused("A different page decrypted the checkpoint") { _ = try b.recoverable() }
            try check(try Data(contentsOf: rb.url) == wrongScope, "Invalid checkpoint was silently replaced")
            var tampered = wrongScope
            tampered[tampered.count - 1] ^= 1
            try tampered.write(to: ra.url, options: .atomic)
            try refused("Tampered ciphertext was accepted") { _ = try a.recoverable() }
            try check(try Data(contentsOf: ra.url) == tampered, "Tampered checkpoint was deleted")
        }

        try run("explicit discard isolates an unreadable checkpoint and preserves the other draft") { root, actor, vault in
            let a = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            let b = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            try a.save(draft("Unreadable edit")); try b.save(draft("Other edit survives"))
            a.release(); b.release()
            let entries = try a.recoverable()
            let bad = try entries.first(where: { $0.draft.blocks[0].text == "Unreadable edit" }).unwrap("Missing checkpoint")
            let good = try entries.first(where: { $0.draft.blocks[0].text == "Other edit survives" }).unwrap("Missing other checkpoint")
            let goodBytes = try Data(contentsOf: good.url)
            var bytes = try Data(contentsOf: bad.url)
            bytes[bytes.count - 1] ^= 1
            try bytes.write(to: bad.url, options: .atomic)
            let failure: PageDraftStore.Unreadable
            do {
                _ = try a.recoverable()
                throw JourneyError.failed("Unreadable checkpoint was accepted")
            } catch let unreadable as PageDraftStore.Unreadable {
                failure = unreadable
            }
            try check(failure.url == bad.url, "Recovery failure did not identify its own checkpoint")
            try check(try Data(contentsOf: bad.url) == bytes, "Reading deleted the unreadable checkpoint")
            let elsewhere = try PageDraftStore(actor: actor, space: "chat", item: "other-page", vault: vault, root: root)
            defer { elsewhere.release() }
            try refused("Discard crossed a page boundary") { try elsewhere.discard(failure) }
            try a.discard(failure)
            try check(try Data(contentsOf: good.url) == goodBytes, "Discard changed another draft")
            let remaining = try a.recoverable()
            try check(remaining.count == 1 && remaining[0].draft.blocks[0].text == "Other edit survives",
                      "Valid draft remained blocked after explicit discard")
        }

        try run("stale discard confirmation cannot delete a newer edit in the same checkpoint") { root, actor, vault in
            let writer = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root)
            try writer.save(draft("Old edit")); writer.release()
            let original = try writer.recoverable()[0]
            var bytes = try Data(contentsOf: original.url)
            bytes[bytes.count - 1] ^= 1
            try bytes.write(to: original.url, options: .atomic)
            let failure: PageDraftStore.Unreadable
            do {
                _ = try writer.recoverable()
                throw JourneyError.failed("Invalid checkpoint was accepted")
            } catch let unreadable as PageDraftStore.Unreadable { failure = unreadable }
            writer.activate()
            try writer.save(draft("New edit after confirmation opened"))
            writer.release()
            try refused("Stale confirmation deleted a newer checkpoint") { try writer.discard(failure) }
            try check(try writer.recoverable()[0].draft.blocks[0].text == "New edit after confirmation opened",
                      "Newer edit was lost after a stale confirmation")
        }

        try run("a new native vault instance recovers the same encrypted checkpoint") { root, actor, _ in
            let defaults = UserDefaults.standard
            let originalDefaults = defaults.volatileDomain(forName: UserDefaults.registrationDomain)
            defaults.register(defaults: ["ZoenDatabasePath": root.appendingPathComponent("page.sqlite").path])
            defer { defaults.setVolatileDomain(originalDefaults, forName: UserDefaults.registrationDomain) }
            let service = "xyz.tironi.zoen.page-draft-test." + UUID().uuidString
            let vault = KeychainVault(service: service)
            let keyName = "zoen-page-editor-" + SHA256.hash(data: Data(actor.utf8)).map { String(format: "%02x", $0) }.joined()
            defer { vault.delete(key: keyName) }
            let a = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: vault, root: root.appendingPathComponent("drafts"))
            try a.save(draft("Native vault relaunch 👩🏽‍💻 雨")); a.release()
            let b = try PageDraftStore(actor: actor, space: "chat", item: "page", vault: KeychainVault(service: service),
                                      root: root.appendingPathComponent("drafts"))
            defer { b.release() }
            try check(try b.recoverable()[0].draft.blocks[0].text == "Native vault relaunch 👩🏽‍💻 雨",
                      "Recreated native vault could not recover the checkpoint")
        }

        try run("native checkpoint survives engine destruction and commits through the real core") { root, _, vault in
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            let path = root.appendingPathComponent("page.sqlite").path
            var engine: ZoenEngine? = try ZoenEngine.open(path: path, locale: "en")
            _ = try engine!.seedDemoIfEmpty()
            let space = try engine!.spaces().first(where: { $0.title == "Paraty with Marina" }).unwrap("Demo chat missing")
            let item = try engine!.pageImportMarkdown(spaceId: space.id, path: "checkpoint.md", markdown: "# Title\n\nOriginal\n")
            let observed = try engine!.page(itemId: item.id)
            let actor = try engine!.me().id
            var blocks = observed.blocks
            blocks[1].text = "Text entered before debounce 👩🏽‍💻 雨"
            let edit = PageEditorDraft(mutationId: "before-debounce", editContext: observed.editContext,
                                       blocks: blocks, baseline: observed.blocks)
            let checkpointRoot = root.appendingPathComponent("drafts")
            let editor = try PageDraftStore(actor: actor, space: space.id, item: item.id, vault: vault, root: checkpointRoot)
            try editor.save(edit)
            editor.release()
            engine = nil
            let reopened = try ZoenEngine.open(path: path, locale: "en")
            let recoveredStore = try PageDraftStore(actor: actor, space: space.id, item: item.id, vault: vault, root: checkpointRoot)
            defer { recoveredStore.release() }
            let recovered = try recoveredStore.recoverable()[0]
            let receipt = try reopened.pageApplyFrom(itemId: item.id, mutationId: recovered.draft.mutationId,
                editContext: recovered.draft.editContext, order: recovered.draft.blocks.map(\.id), changed: recovered.draft.changed)
            try check(receipt.page.blocks == blocks, "The real core did not apply the recovered checkpoint")
            try recoveredStore.acknowledge(recovered)
            try check(try recoveredStore.recoverable().isEmpty, "Accepted checkpoint remained orphaned")
            let replay = try reopened.pageApplyFrom(itemId: item.id, mutationId: edit.mutationId,
                editContext: edit.editContext, order: edit.blocks.map(\.id), changed: edit.changed)
            try check(replay.replayed, "Lost native acknowledgement duplicated the core edit")
            try check(try reopened.pageCommit(itemId: item.id, note: "Recovered checkpoint"), "Recovered edit did not commit")
            let saved = try reopened.page(itemId: item.id)
            try check(saved.version == 2 && !saved.unsaved, "Recovered edit did not become a saved version")
            try check(try reopened.pageMarkdown(itemId: item.id).contains(blocks[1].text), "Unicode text was lost on export")
        }
        print("PASS 10 checkpoint journeys (real CryptoKit/filesystem/core; one native vault, nine memory vault cases)")
    }
}

private extension Optional {
    func unwrap(_ message: String) throws -> Wrapped {
        guard let value = self else { throw JourneyError.failed(message) }
        return value
    }
}
