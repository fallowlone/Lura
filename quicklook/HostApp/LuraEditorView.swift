import AppKit
import SwiftUI

private let previewDebounceNs: UInt64 = 300_000_000

struct LuraEditorContainer: View {
    let url: URL
    let onClose: () -> Void

    @State private var document: LuraFileDocument?
    @State private var loadError: String?

    var body: some View {
        Group {
            if let err = loadError {
                VStack(spacing: 16) {
                    Text("Could not open file")
                        .font(.headline)
                    Text(err)
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    Button("Back") { onClose() }
                        .keyboardShortcut(.escape, modifiers: [])
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(32)
            } else if let doc = document {
                LuraEditorView(document: doc, onClose: onClose)
            } else {
                ProgressView("Loading…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .onAppear(perform: loadDocument)
            }
        }
    }

    private func loadDocument() {
        do {
            document = try LuraFileDocument(url: url)
        } catch {
            loadError = error.localizedDescription
        }
    }
}

struct LuraEditorView: View {
    @EnvironmentObject private var appModel: LuraAppModel
    @ObservedObject var document: LuraFileDocument
    let onClose: () -> Void

    @State private var previewPDFData: Data?
    @State private var previewError: String?
    @State private var showPreview: Bool = true
    @State private var debounceTask: Task<Void, Never>?

    var body: some View {
        NavigationStack {
            Group {
                if showPreview {
                    HSplitView {
                        editorPane
                            .frame(minWidth: 280)
                        previewPane
                            .frame(minWidth: 240)
                    }
                } else {
                    editorPane
                }
            }
            .background(Color(nsColor: .windowBackgroundColor))
            .navigationTitle(windowTitle)
            .toolbar {
                ToolbarItemGroup(placement: .navigation) {
                    Button {
                        attemptClose()
                    } label: {
                        Label("Close", systemImage: "chevron.backward")
                    }
                    .keyboardShortcut(.escape, modifiers: [])
                    .help("Return to welcome screen")
                }

                ToolbarItemGroup(placement: .primaryAction) {
                    Button {
                        saveDocument()
                    } label: {
                        Label("Save", systemImage: "square.and.arrow.down")
                    }
                    .keyboardShortcut("s", modifiers: [.command])
                    .disabled(!document.isDirty)

                    Button {
                        openAnotherDocument()
                    } label: {
                        Label("Open…", systemImage: "folder")
                    }
                    .keyboardShortcut("o", modifiers: [.command])

                    Button {
                        revertDocument()
                    } label: {
                        Label("Revert", systemImage: "arrow.uturn.backward")
                    }
                    .disabled(!document.isDirty)

                    Toggle(isOn: $showPreview) {
                        Label("Preview", systemImage: "eye")
                    }
                    .help("Show or hide PDF preview (same pipeline as export)")
                }
            }
        }
        .onAppear {
            appModel.editorIsDirty = document.isDirty
            applyPreviewOutput(LuraRenderFFI.renderPDF(source: document.text))
        }
        .onChange(of: document.text) { newValue in
            appModel.editorIsDirty = document.isDirty
            debounceTask?.cancel()
            debounceTask = Task { @MainActor in
                try? await Task.sleep(nanoseconds: previewDebounceNs)
                guard !Task.isCancelled else { return }
                applyPreviewOutput(LuraRenderFFI.renderPDF(source: newValue))
            }
        }
        .onDisappear {
            debounceTask?.cancel()
            appModel.editorIsDirty = false
        }
    }

    private var windowTitle: String {
        let name = document.url.lastPathComponent
        return document.isDirty ? "\(name) ·" : name
    }

    private var editorPane: some View {
        TextEditor(text: $document.text)
            .font(.system(.body, design: .monospaced))
            .scrollContentBackground(.hidden)
            .background(Color(nsColor: .textBackgroundColor))
            .padding(8)
    }

    private var previewPane: some View {
        ZStack {
            PDFPreviewRepresentable(pdfData: previewPDFData)
            if let err = previewError {
                ScrollView {
                    Text(err)
                        .font(.system(.body, design: .monospaced))
                        .foregroundStyle(.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                }
                .background(Color(nsColor: .textBackgroundColor).opacity(0.92))
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
        )
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private func applyPreviewOutput(_ out: LuraPdfFFI.Output) {
        previewPDFData = out.pdfData
        previewError = out.errorMessage
    }

    private func saveDocument() {
        do {
            try document.save()
            appModel.editorIsDirty = false
        } catch {
            presentAlert(title: "Save failed", message: error.localizedDescription)
        }
    }

    private func revertDocument() {
        do {
            try document.revert()
            appModel.editorIsDirty = false
            applyPreviewOutput(LuraRenderFFI.renderPDF(source: document.text))
        } catch {
            presentAlert(title: "Revert failed", message: error.localizedDescription)
        }
    }

    private func openAnotherDocument() {
        guard !document.isDirty || confirmDiscardForOpen() else { return }
        appModel.presentOpenDocumentReplacingCurrent()
    }

    private func confirmDiscardForOpen() -> Bool {
        let alert = NSAlert()
        alert.messageText = "Discard unsaved changes?"
        alert.informativeText = "Opening another file will close the current document."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Discard")
        return alert.runModal() == .alertSecondButtonReturn
    }

    private func attemptClose() {
        guard document.isDirty else {
            onClose()
            return
        }
        let alert = NSAlert()
        alert.messageText = "Save changes to “\(document.url.lastPathComponent)”?"
        alert.informativeText = "Your changes will be lost if you do not save."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Don’t Save")
        alert.addButton(withTitle: "Cancel")
        let response = alert.runModal()
        switch response {
        case .alertFirstButtonReturn:
            do {
                try document.save()
                onClose()
            } catch {
                presentAlert(title: "Save failed", message: error.localizedDescription)
            }
        case .alertSecondButtonReturn:
            onClose()
        default:
            break
        }
    }

    private func presentAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.runModal()
    }
}
