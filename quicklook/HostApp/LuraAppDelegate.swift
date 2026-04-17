import AppKit

/// Apps that declare document types get a second "untitled" window by default; we only use SwiftUI `WindowGroup`.
final class LuraAppDelegate: NSObject, NSApplicationDelegate {
    /// URLs received from Launch Services before `LuraAppModel` registered itself.
    private var pendingURLs: [URL] = []
    private weak var model: LuraAppModel?

    func applicationShouldOpenUntitledFile(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        LuraDebugLog.log("applicationDidFinishLaunching")
        if let path = LuraDebugLog.fileURL?.path {
            LuraDebugLog.log("debug file (tail -f): \(path)")
        } else {
            LuraDebugLog.log("debug file: unavailable (Caches URL missing)")
        }
        LuraDebugLog.log(
            "live log: log stream --style compact --info --predicate 'subsystem == \"\(LuraDebugLog.subsystem)\"'"
        )
        #if DEBUG
        SecurityScopedURL.runSelfTest()
        #endif
    }

    /// Called by Launch Services on cold launch and by Finder double-click while running.
    /// SwiftUI also fires `WindowGroup.onOpenURL` for the warm case; the appModel dedupes.
    func application(_ application: NSApplication, open urls: [URL]) {
        LuraDebugLog.log("application(_:open:) urls=\(urls.map { $0.lastPathComponent })")
        if let model = model {
            for url in urls { model.openDocumentURL(url) }
        } else {
            pendingURLs.append(contentsOf: urls)
        }
    }

    @MainActor
    func register(model: LuraAppModel) {
        self.model = model
        if !pendingURLs.isEmpty {
            LuraDebugLog.log("AppDelegate: draining \(pendingURLs.count) pending URLs")
            let drained = pendingURLs
            pendingURLs.removeAll()
            for url in drained { model.openDocumentURL(url) }
        }
    }
}
