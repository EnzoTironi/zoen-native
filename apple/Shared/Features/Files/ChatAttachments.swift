import SwiftUI
import PhotosUI
import CoreTransferable
import UniformTypeIdentifiers
import RodaCore

private struct PickedAttachmentPhoto: Transferable, Sendable {
    let file: AttachmentFile

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { received in
            let files = try AttachmentFiles.stage([received.file])
            return Self(file: files[0])
        }
    }
}

/// The composer owns only this button. Selection, private review and sending stay here.
struct ChatAttachmentButton: View {
    @Environment(AppModel.self) private var model
    let spaceId: String
    @State private var importing = false
    @State private var pickingPhoto = false
    @State private var selectedPhoto: PhotosPickerItem?
    @State private var cameraOpen = false
    @State private var preparing = false
    @State private var preparationTask: Task<Void, Never>?
    @State private var capturedPhoto: Data?
    @State private var draft: AttachmentDraft?
    @State private var reviewing = false
    @State private var failure: String?

    var body: some View {
        Menu {
            #if os(iOS)
            Button("Take photo", systemImage: "camera") { cameraOpen = true }
            #endif
            Button("Photo library", systemImage: "photo.on.rectangle") { pickingPhoto = true }
            Button("Choose files", systemImage: "folder") { importing = true }
        } label: {
            Group {
                if preparing { ProgressView().controlSize(.small) }
                else { NativeIconMotion(systemName: "plus", size: 19) }
            }
            .frame(width: 44, height: 44)
            .foregroundStyle(Palette.textPrimary)
        }
        .menuStyle(.borderlessButton)
        .glassEffect(.regular.interactive(), in: .circle)
        .disabled(preparing)
        .accessibilityLabel("Attach photo or file")
        .accessibilityIdentifier("chat.attach")
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            switch result {
            case .success(let urls): prepare(urls)
            case .failure(let error): failure = error.localizedDescription
            }
        }
        .photosPicker(isPresented: $pickingPhoto, selection: $selectedPhoto, matching: .images,
                      preferredItemEncoding: .current)
        .onChange(of: selectedPhoto) { _, photo in
            guard let photo else { return }
            preparing = true
            preparationTask = Task {
                defer { preparing = false; selectedPhoto = nil }
                do {
                    guard let imported = try await photo.loadTransferable(type: PickedAttachmentPhoto.self) else {
                        throw CocoaError(.fileReadCorruptFile)
                    }
                    do { try Task.checkCancellation() }
                    catch { imported.file.discard(); throw error }
                    showReview([imported.file])
                } catch is CancellationError {
                } catch { failure = error.localizedDescription }
            }
        }
        #if os(iOS)
        .sheet(isPresented: $cameraOpen, onDismiss: prepareCapturedPhoto) {
            NavigationStack {
                InlineCameraPanel(onClose: { cameraOpen = false }) { bytes in
                    capturedPhoto = bytes
                    cameraOpen = false
                }
                .padding(12)
                .navigationTitle("Take photo")
                .navigationBarTitleDisplayMode(.inline)
            }
            .zoenSheet([.medium, .large])
        }
        #endif
        .sheet(isPresented: $reviewing, onDismiss: closeReview) {
            if let draft {
                AttachmentReviewSheet(spaceId: spaceId, files: draft.files)
                    .environment(model)
            }
        }
        .onDisappear {
            preparationTask?.cancel()
            capturedPhoto = nil
            closeReview()
        }
        .alert("Could not attach", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) { failure = nil }
        } message: { Text(failure ?? "") }
    }

    private func prepare(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        preparing = true
        preparationTask = Task {
            defer { preparing = false }
            do {
                let files = try await AttachmentFiles.importSelection(urls)
                do { try Task.checkCancellation() }
                catch { files.forEach { $0.discard() }; throw error }
                showReview(files)
            } catch is CancellationError {
            } catch { failure = error.localizedDescription }
        }
    }

    private func prepareCapturedPhoto() {
        guard let bytes = capturedPhoto else { return }
        capturedPhoto = nil
        preparing = true
        preparationTask = Task {
            defer { preparing = false }
            do {
                let name = "Photo-" + Date.now.formatted(.iso8601).replacingOccurrences(of: ":", with: "-") + ".jpg"
                let work = Task.detached(priority: .userInitiated) { try AttachmentFiles.stage(bytes, name: name) }
                let file = try await withTaskCancellationHandler {
                    try await work.value
                } onCancel: { work.cancel() }
                do { try Task.checkCancellation() }
                catch { file.discard(); throw error }
                showReview([file])
            } catch is CancellationError {
            } catch { failure = error.localizedDescription }
        }
    }

    private func showReview(_ files: [AttachmentFile]) {
        draft?.discard()
        draft = AttachmentDraft(files: files)
        reviewing = true
    }

    private func closeReview() {
        draft?.discard()
        draft = nil
        reviewing = false
    }
}

private struct AttachmentReviewSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let spaceId: String
    @State var files: [AttachmentFile]
    @State private var sending = false
    @State private var failure: String?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(files) { file in
                        HStack(spacing: 12) {
                            NavigationLink {
                                AttachmentPreviewScreen(file: file)
                            } label: {
                                HStack(spacing: 12) {
                                    AttachmentDraftThumbnail(file: file)
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(file.name).font(.body.weight(.medium)).lineLimit(2)
                                        Text(ByteCountFormatter.string(fromByteCount: file.byteCount, countStyle: .file))
                                            .font(.caption).foregroundStyle(Palette.textSecondary)
                                        Text("Preview").font(.caption).foregroundStyle(Palette.action)
                                    }
                                }
                            }
                            .disabled(sending)
                            .accessibilityIdentifier("chat.attachment.preview")
                            Spacer(minLength: 0)
                            if files.count > 1 {
                                Button(role: .destructive) {
                                    file.discard()
                                    files.removeAll { $0.id == file.id }
                                } label: {
                                    Image(systemName: "minus.circle.fill").frame(width: 44, height: 44)
                                }
                                .buttonStyle(.plain)
                                .disabled(sending)
                                .accessibilityLabel(String(localized: "Remove \(file.name)"))
                            }
                        }
                        .padding(.vertical, 4)
                    }
                } footer: {
                    Text("Shared in this conversation. Files stay on this device until you send.")
                }
                if let failure {
                    Section { Text(failure).foregroundStyle(Palette.danger) }
                }
            }
            .navigationTitle("Send attachments")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        files.forEach { $0.discard() }
                        dismiss()
                    }
                        .disabled(sending)
                        .keyboardShortcut(.cancelAction)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        send()
                    } label: {
                        if sending { ProgressView().controlSize(.small) }
                        else { Text("Send") }
                    }
                    .disabled(sending || files.isEmpty)
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("chat.attachment.send")
                }
            }
        }
        .interactiveDismissDisabled(sending)
        #if os(macOS)
        .frame(minWidth: 420, idealWidth: 500, minHeight: 320)
        #endif
        .zoenSheet([.medium, .large])
    }

    private func send() {
        guard !sending else { return }
        sending = true
        failure = nil
        Task {
            defer { sending = false }
            do {
                while let file = files.first {
                    try await FileSupport.sendAttachment(file, into: spaceId, model: model)
                    file.discard()
                    files.removeFirst()
                }
                Haptics.commit()
                dismiss()
            } catch { failure = (error as? CoreError)?.message ?? error.localizedDescription }
        }
    }
}

private struct AttachmentPreviewScreen: View {
    let file: AttachmentFile

    var body: some View {
        QuickLookView(file: file)
            .navigationTitle(file.name)
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
    }
}

private struct AttachmentDraftThumbnail: View {
    let file: AttachmentFile
    @State private var image: PlatformImage?

    var body: some View {
        Group {
            if let image {
                #if canImport(UIKit)
                Image(uiImage: image).resizable().scaledToFill()
                #else
                Image(nsImage: image).resizable().scaledToFill()
                #endif
            } else {
                Image(systemName: FileSupport.symbol(mime: FileSupport.mime(for: file.url)))
                    .font(.title2).foregroundStyle(Palette.textSecondary)
            }
        }
        .frame(width: 68, height: 68)
        .clipShape(.rect(cornerRadius: 12))
        .accessibilityHidden(true)
        .task(id: file.id) {
            if let data = await FileSupport.thumbnail(for: file.url) { image = PlatformImage(data: data) }
        }
    }
}
