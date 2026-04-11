import AppKit
import SwiftUI
import UniformTypeIdentifiers

@MainActor
final class LuraAppModel: ObservableObject {
    @Published var openEditorURL: URL?
    /// Synced from the editor so menu actions can warn before replacing an unsaved document.
    @Published var editorIsDirty: Bool = false

    /// Single exported UTI from HostInfo.plist (`fol` + `lura` extensions on one type).
    private static var documentTypes: [UTType] {
        [UTType(exportedAs: "com.fallowlone.lura-document")]
    }

    /// SwiftUI + AppKit: `NSSavePanel.runModal()` often returns cancel (0) immediately; sheet works reliably.
    private static func windowForSheet() -> NSWindow? {
        NSApp.keyWindow
            ?? NSApp.mainWindow
            ?? NSApp.windows.first(where: { $0.isVisible && !$0.isSheet })
    }

    func presentNewDocument() {
        LuraDebugLog.log("presentNewDocument() entered")
        LuraDebugLog.log(
            "state: openEditorURL=\(openEditorURL?.path ?? "nil") editorIsDirty=\(editorIsDirty)"
        )
        if !mayReplaceOpenDocument() {
            LuraDebugLog.log("presentNewDocument: mayReplaceOpenDocument returned false, abort")
            return
        }

        let types = Self.documentTypes
        LuraDebugLog.log("NSSavePanel: allowedContentTypes count=\(types.count)")
        for (i, t) in types.enumerated() {
            LuraDebugLog.log(
                "  type[\(i)] identifier=\(t.identifier) preferredFilenameExtension=\(t.preferredFilenameExtension ?? "nil")"
            )
        }

        Task { @MainActor in
            await Task.yield()
            let panel = NSSavePanel()
            panel.title = "New Lura document"
            // Match preferred extension from UTI so the panel does not reject the default name.
            panel.nameFieldStringValue = "Untitled.lura"
            panel.allowedContentTypes = types
            panel.canCreateDirectories = true
            panel.isExtensionHidden = false

            if let window = Self.windowForSheet() {
                LuraDebugLog.log("NSSavePanel: beginSheetModal for window=\(window.title)")
                panel.beginSheetModal(for: window) { [weak self] response in
                    guard let self else { return }
                    Task { @MainActor in
                        self.finishNewDocument(panel: panel, response: response)
                    }
                }
            } else {
                LuraDebugLog.log("NSSavePanel: no NSWindow, runModal fallback windows=\(NSApp.windows.count)")
                let response = panel.runModal()
                finishNewDocument(panel: panel, response: response)
            }
        }
    }

    private func finishNewDocument(panel: NSSavePanel, response: NSApplication.ModalResponse) {
        LuraDebugLog.log(
            "NSSavePanel: finished rawValue=\(response.rawValue) (.OK=\(NSApplication.ModalResponse.OK.rawValue)) url=\(panel.url?.path ?? "nil")"
        )
        guard response == .OK, let url = panel.url else {
            LuraDebugLog.log("NSSavePanel: cancelled or no url")
            return
        }
        do {
            try LuraTemplates.newDocument.write(to: url, atomically: true, encoding: .utf8)
            LuraDebugLog.log("write template OK, opening editor")
            RecentFilesStore.shared.recordOpened(url)
            openEditorURL = url
            editorIsDirty = false
        } catch {
            LuraDebugLog.log("write template FAILED: \(error.localizedDescription)")
            presentAlert(title: "Could not create file", message: error.localizedDescription)
        }
    }

    func presentOpenDocument() {
        if !mayReplaceOpenDocument() { return }

        Task { @MainActor in
            await Task.yield()
            let panel = NSOpenPanel()
            panel.title = "Open Lura document"
            panel.allowedContentTypes = Self.documentTypes
            panel.allowsMultipleSelection = false
            panel.canChooseDirectories = false
            panel.canChooseFiles = true

            if let window = Self.windowForSheet() {
                panel.beginSheetModal(for: window) { [weak self] response in
                    guard let self else { return }
                    Task { @MainActor in
                        self.finishOpen(panel: panel, response: response)
                    }
                }
            } else {
                let response = panel.runModal()
                finishOpen(panel: panel, response: response)
            }
        }
    }

    func presentOpenDocumentReplacingCurrent() {
        Task { @MainActor in
            await Task.yield()
            let panel = NSOpenPanel()
            panel.title = "Open Lura document"
            panel.allowedContentTypes = Self.documentTypes
            panel.allowsMultipleSelection = false
            panel.canChooseDirectories = false
            panel.canChooseFiles = true

            if let window = Self.windowForSheet() {
                panel.beginSheetModal(for: window) { [weak self] response in
                    guard let self else { return }
                    Task { @MainActor in
                        self.finishOpen(panel: panel, response: response)
                    }
                }
            } else {
                let response = panel.runModal()
                finishOpen(panel: panel, response: response)
            }
        }
    }

    private func finishOpen(panel: NSOpenPanel, response: NSApplication.ModalResponse) {
        guard response == .OK, let url = panel.url else { return }
        RecentFilesStore.shared.recordOpened(url)
        openEditorURL = url
        editorIsDirty = false
    }

    func openDocumentURL(_ url: URL) {
        if !mayReplaceOpenDocument() { return }
        RecentFilesStore.shared.recordOpened(url)
        openEditorURL = url
        editorIsDirty = false
    }

    private func mayReplaceOpenDocument() -> Bool {
        guard openEditorURL != nil, editorIsDirty else { return true }
        let alert = NSAlert()
        alert.messageText = "Discard unsaved changes?"
        alert.informativeText = "The document in the editor has not been saved."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Discard")
        return alert.runModal() == .alertSecondButtonReturn
    }

    private func presentAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.runModal()
    }
}
