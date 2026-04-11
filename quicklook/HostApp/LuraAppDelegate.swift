import AppKit

/// Apps that declare document types get a second "untitled" window by default; we only use SwiftUI `WindowGroup`.
final class LuraAppDelegate: NSObject, NSApplicationDelegate {
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
    }
}
