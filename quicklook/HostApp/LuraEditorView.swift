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

    // Find (Cmd+F)
    @State private var findQuery: String = ""
    @State private var isFindBarVisible: Bool = false
    @State private var textUnits: [LuraTextUnit] = []
    @State private var currentMatchIndex: Int = 0
    @FocusState private var findFieldFocused: Bool

    /// Units matching `findQuery`. Kept in state and refreshed only when the query or the
    /// index changes, instead of re-filtering `textUnits` on every access during a render.
    @State private var matches: [LuraTextUnit] = []

    private func recomputeMatches() {
        guard !findQuery.isEmpty else {
            matches = []
            return
        }
        let needle = findQuery.lowercased()
        matches = textUnits.filter { $0.text.lowercased().contains(needle) }
    }

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
                        attemptEscape()
                    } label: {
                        Label("Close", systemImage: "chevron.backward")
                    }
                    .keyboardShortcut(.escape, modifiers: [])
                    .help("Return to welcome screen (closes find bar first)")
                }

                ToolbarItemGroup(placement: .primaryAction) {
                    Button {
                        toggleFindBar()
                    } label: {
                        Label("Find", systemImage: "magnifyingglass")
                    }
                    .keyboardShortcut("f", modifiers: [.command])
                    .help("Find in document")

                    Button("Next Match") {
                        cycleMatch(forward: true)
                    }
                    .keyboardShortcut("g", modifiers: [.command])
                    .hidden()
                    .frame(width: 0, height: 0)

                    Button("Previous Match") {
                        cycleMatch(forward: false)
                    }
                    .keyboardShortcut("g", modifiers: [.command, .shift])
                    .hidden()
                    .frame(width: 0, height: 0)

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
            refreshTextIndex(source: document.text)
        }
        .onChange(of: document.text) { newValue in
            appModel.editorIsDirty = document.isDirty
            debounceTask?.cancel()
            debounceTask = Task { @MainActor in
                try? await Task.sleep(nanoseconds: previewDebounceNs)
                guard !Task.isCancelled else { return }
                applyPreviewOutput(LuraRenderFFI.renderPDF(source: newValue))
                refreshTextIndex(source: newValue)
            }
        }
        .onChange(of: findQuery) { _ in
            recomputeMatches()
            currentMatchIndex = 0
        }
        .onDisappear {
            debounceTask?.cancel()
            appModel.editorIsDirty = false
        }
    }

    private func toggleFindBar() {
        isFindBarVisible.toggle()
        if isFindBarVisible {
            findFieldFocused = true
        } else {
            findQuery = ""
        }
    }

    private func cycleMatch(forward: Bool) {
        let all = matches
        guard !all.isEmpty else { return }
        if forward {
            currentMatchIndex = (currentMatchIndex + 1) % all.count
        } else {
            currentMatchIndex = (currentMatchIndex - 1 + all.count) % all.count
        }
    }

    private func refreshTextIndex(source: String) {
        switch LuraRenderFFI.extractText(source: source) {
        case .success(let units):
            textUnits = units
            recomputeMatches()
            if currentMatchIndex >= matches.count {
                currentMatchIndex = 0
            }
        case .failure:
            // Parse errors are surfaced via the preview pane; leave index stale.
            break
        }
    }

    @ViewBuilder
    private var findBar: some View {
        if isFindBarVisible {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField("Find in document", text: $findQuery)
                    .textFieldStyle(.plain)
                    .focused($findFieldFocused)
                    .onSubmit { cycleMatch(forward: true) }
                Text(matchCountLabel)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Button {
                    cycleMatch(forward: false)
                } label: {
                    Image(systemName: "chevron.up")
                }
                .buttonStyle(.borderless)
                .disabled(matches.isEmpty)
                Button {
                    cycleMatch(forward: true)
                } label: {
                    Image(systemName: "chevron.down")
                }
                .buttonStyle(.borderless)
                .disabled(matches.isEmpty)
                Button {
                    isFindBarVisible = false
                    findQuery = ""
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.borderless)
                .help("Close find bar (Esc)")
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor))
                    .overlay(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
                    )
            )
            .padding(.horizontal, 12)
            .padding(.top, 8)
        }
    }

    private var matchCountLabel: String {
        let total = matches.count
        if total == 0 { return findQuery.isEmpty ? "" : "0" }
        return "\(currentMatchIndex + 1) / \(total)"
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
        VStack(spacing: 0) {
            findBar
            ZStack {
                PDFPreviewRepresentable(
                    pdfData: previewPDFData,
                    matches: matches,
                    currentMatchIndex: currentMatchIndex
                )
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
        }
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private func applyPreviewOutput(_ out: LuraPdfFFI.Output) {
        previewPDFData = out.pdfData
        previewError = out.errorMessage
        if let pdf = out.pdfData, let utf8 = document.text.data(using: .utf8) {
            LuraPreviewDiskCache.store(pdf, forDocumentData: utf8)
        }
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

    /// Esc handler shared by the toolbar Close button and the find bar. If the
    /// find bar is visible, Esc dismisses it first without navigating away;
    /// otherwise behaves like the old "back to welcome" action.
    private func attemptEscape() {
        if isFindBarVisible {
            isFindBarVisible = false
            findQuery = ""
            return
        }
        attemptClose()
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
