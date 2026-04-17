# Phase A — File-Open and Bookmarks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Double-click `.lura` in Finder opens the editor directly. Recents reopen across reboot without permission prompt.

**Architecture:** Wire SwiftUI `WindowGroup.onOpenURL` and `NSApplicationDelegate.application(_:open:)` to route file URLs into `LuraAppModel.openDocumentURL`. Replace raw `[URL]` persistence in `RecentFilesStore` with security-scoped bookmark blobs. Track scope start/stop on the open document via a small RAII helper.

**Tech Stack:** Swift 5.9, SwiftUI (macOS 13+), AppKit (`NSApplicationDelegate`, `NSOpenPanel`), Foundation `URL.bookmarkData(options: .withSecurityScope)`. Built via `install-preview.sh` (`swiftc` direct compile, no Xcode project).

**Spec:** `docs/superpowers/specs/2026-04-17-ui-perf-fileopen-design.md` (Phase A section).

**Test infrastructure note:** Project has no Swift test target today (Rust tests in `tests/*.rs`, no `XCTest`). Bootstrapping `swift test` is out of scope for Phase A. Verification uses `LuraDebugLog` assertions and a manual smoke checklist. Bookmark roundtrip is the highest-risk piece — Task 2 includes a runtime self-test that fires on first launch in DEBUG builds and writes pass/fail to `LuraDebugLog`.

---

## File Structure

| File                                        | Action | Responsibility                                                                                                            |
| ------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------- |
| `quicklook/HostApp/RecentEntry.swift`       | create | `Codable` struct: `bookmark: Data`, `displayPath: String`, `lastOpened: Date`                                             |
| `quicklook/HostApp/SecurityScopedURL.swift` | create | RAII wrapper around a URL: `start()`, `stop()`, balanced refcount                                                         |
| `quicklook/HostApp/RecentFilesStore.swift`  | modify | Switch from `[URL]` strings in `UserDefaults` to `[RecentEntry]` JSON blob; expose resolve API                            |
| `quicklook/HostApp/LuraAppDelegate.swift`   | modify | Implement `application(_:open urls:)`, queue URLs received before model registers                                         |
| `quicklook/HostApp/LuraApp.swift`           | modify | Add `.onOpenURL { url in appModel.openDocumentURL(url) }` to `WindowGroup`                                                |
| `quicklook/HostApp/LuraAppModel.swift`      | modify | New `openDocumentURL(bookmark:)` overload; track scope on open document; expose `registerWithDelegate(_:)` to drain queue |
| `quicklook/HostApp/LuraFileDocument.swift`  | modify | Hold `SecurityScopedURL?`, stop on `deinit`                                                                               |
| `quicklook/HostApp/WelcomeView.swift`       | modify | `RecentRow` consumes `RecentEntry` instead of `URL`; gracefully handle stale bookmarks                                    |
| `install-preview.sh`                        | modify | Add `RecentEntry.swift` and `SecurityScopedURL.swift` to `HOST_SWIFT` array                                               |

---

## Task 1: `RecentEntry` model

**Files:**

- Create: `quicklook/HostApp/RecentEntry.swift`
- Modify: `install-preview.sh` (add file to `HOST_SWIFT`)

- [ ] **Step 1: Create the model file**

```swift
// quicklook/HostApp/RecentEntry.swift
import Foundation

struct RecentEntry: Codable, Equatable {
    var bookmark: Data
    var displayPath: String
    var lastOpened: Date
}
```

- [ ] **Step 2: Add file to build script**

In `install-preview.sh`, locate the `HOST_SWIFT=(` block (around line 35). Insert this line **before** `quicklook/HostApp/RecentFilesStore.swift`:

```bash
    quicklook/HostApp/RecentEntry.swift
```

- [ ] **Step 3: Build to verify it compiles**

Run: `bash install-preview.sh`
Expected: build succeeds without referencing `RecentEntry` errors. Warnings about unused type are OK (it gets used in Task 3).

- [ ] **Step 4: Commit**

```bash
git add quicklook/HostApp/RecentEntry.swift install-preview.sh
git commit -m "feat(host): add RecentEntry model for bookmark-based recents"
```

---

## Task 2: `SecurityScopedURL` RAII wrapper

**Files:**

- Create: `quicklook/HostApp/SecurityScopedURL.swift`
- Modify: `install-preview.sh`

- [ ] **Step 1: Create the wrapper**

```swift
// quicklook/HostApp/SecurityScopedURL.swift
import Foundation

/// Balances startAccessingSecurityScopedResource / stop calls.
/// Sandbox enforces that every successful start MUST be paired with a stop.
final class SecurityScopedURL {
    let url: URL
    private var didStart: Bool = false

    init(url: URL) {
        self.url = url
    }

    /// Returns true if access was granted. Safe to call multiple times; only the first start is honoured.
    @discardableResult
    func start() -> Bool {
        guard !didStart else { return true }
        didStart = url.startAccessingSecurityScopedResource()
        return didStart
    }

    func stop() {
        guard didStart else { return }
        url.stopAccessingSecurityScopedResource()
        didStart = false
    }

    deinit {
        stop()
    }
}
```

- [ ] **Step 2: Add file to build script**

In `install-preview.sh`, in the `HOST_SWIFT=(` block, insert before `RecentFilesStore.swift`:

```bash
    quicklook/HostApp/SecurityScopedURL.swift
```

- [ ] **Step 3: Add a runtime self-test in DEBUG**

Append this to `quicklook/HostApp/SecurityScopedURL.swift`:

```swift
#if DEBUG
extension SecurityScopedURL {
    /// Sanity self-test: writes a tmp file, takes a security-scoped bookmark on it,
    /// resolves the bookmark, and verifies start/stop balance. Logs result.
    static func runSelfTest() {
        let log = { (msg: String) in LuraDebugLog.log("SecurityScopedURL.selfTest: \(msg)") }
        let tmpURL = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("lura-scope-selftest-\(UUID().uuidString).txt")
        do {
            try "hello".write(to: tmpURL, atomically: true, encoding: .utf8)
            let bookmark = try tmpURL.bookmarkData(
                options: [.withSecurityScope],
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
            var stale = false
            let resolved = try URL(
                resolvingBookmarkData: bookmark,
                options: [.withSecurityScope],
                relativeTo: nil,
                bookmarkDataIsStale: &stale
            )
            let scope = SecurityScopedURL(url: resolved)
            let started = scope.start()
            let data = try? Data(contentsOf: resolved)
            scope.stop()
            try? FileManager.default.removeItem(at: tmpURL)
            if started, data == "hello".data(using: .utf8) {
                log("PASS (stale=\(stale))")
            } else {
                log("FAIL started=\(started) dataNil=\(data == nil)")
            }
        } catch {
            log("ERROR \(error.localizedDescription)")
        }
    }
}
#endif
```

- [ ] **Step 4: Wire the self-test into `applicationDidFinishLaunching`**

In `quicklook/HostApp/LuraAppDelegate.swift`, inside `applicationDidFinishLaunching(_:)`, after the existing log lines, append:

```swift
        #if DEBUG
        SecurityScopedURL.runSelfTest()
        #endif
```

- [ ] **Step 5: Build and run; tail the debug log**

Run: `bash install-preview.sh && open ~/Applications/Lura.app`
Then in another terminal:
`tail -n 20 "$HOME/Library/Containers/com.fallowlone.lura-document-app/Data/Library/Caches/LuraDebug/ui.log"`
Expected: line containing `SecurityScopedURL.selfTest: PASS`.

- [ ] **Step 6: Commit**

```bash
git add quicklook/HostApp/SecurityScopedURL.swift quicklook/HostApp/LuraAppDelegate.swift install-preview.sh
git commit -m "feat(host): RAII security-scoped URL wrapper with debug self-test"
```

---

## Task 3: `RecentFilesStore` — switch to bookmarks

**Files:**

- Modify: `quicklook/HostApp/RecentFilesStore.swift`

- [ ] **Step 1: Replace the file in full**

Overwrite `quicklook/HostApp/RecentFilesStore.swift` with:

```swift
import Foundation
import SwiftUI

@MainActor
final class RecentFilesStore: ObservableObject {
    static let shared = RecentFilesStore()

    /// Resolved entries. `bookmark` is opaque; consumers use `resolve(_:)` to obtain a live URL.
    @Published private(set) var entries: [RecentEntry] = []

    /// Convenience for the legacy code path that only needs display URLs (read-only listing).
    var urls: [URL] { entries.map { URL(fileURLWithPath: $0.displayPath) } }

    private let defaultsKey = "com.fallowlone.lura-document-app.recentFiles.v2"
    private let legacyDefaultsKey = "com.fallowlone.lura-document-app.recentFiles"
    private let maxEntries = 12

    private init() {
        refreshFromDisk()
    }

    // MARK: - Persistence

    func refreshFromDisk() {
        if let data = UserDefaults.standard.data(forKey: defaultsKey),
           let decoded = try? JSONDecoder().decode([RecentEntry].self, from: data) {
            entries = decoded
            return
        }
        // One-time migration from v1 raw paths. Drop bookmarks not regenerable.
        if let legacy = UserDefaults.standard.stringArray(forKey: legacyDefaultsKey) {
            entries = []
            for path in legacy {
                let url = URL(fileURLWithPath: path)
                guard FileManager.default.fileExists(atPath: url.path) else { continue }
                if let bm = try? url.bookmarkData(
                    options: [.withSecurityScope],
                    includingResourceValuesForKeys: nil,
                    relativeTo: nil
                ) {
                    entries.append(RecentEntry(bookmark: bm, displayPath: url.path, lastOpened: Date()))
                }
            }
            persist()
            UserDefaults.standard.removeObject(forKey: legacyDefaultsKey)
            return
        }
        entries = []
    }

    private func persist() {
        if let data = try? JSONEncoder().encode(entries) {
            UserDefaults.standard.set(data, forKey: defaultsKey)
        }
    }

    // MARK: - Mutation

    /// Records that `url` was opened. Caller already has live access (Powerbox or just-resolved bookmark).
    func recordOpened(_ url: URL) {
        let bookmark: Data
        do {
            bookmark = try url.bookmarkData(
                options: [.withSecurityScope],
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
        } catch {
            LuraDebugLog.log("RecentFilesStore.recordOpened: bookmarkData failed \(error.localizedDescription)")
            return
        }
        entries.removeAll { $0.displayPath == url.path }
        entries.insert(
            RecentEntry(bookmark: bookmark, displayPath: url.path, lastOpened: Date()),
            at: 0
        )
        if entries.count > maxEntries {
            entries = Array(entries.prefix(maxEntries))
        }
        persist()
    }

    func remove(_ entry: RecentEntry) {
        entries.removeAll { $0 == entry }
        persist()
    }

    func clearAll() {
        entries = []
        UserDefaults.standard.removeObject(forKey: defaultsKey)
    }

    // MARK: - Resolution

    enum ResolveError: Error {
        case missing
        case bookmarkInvalid(Error)
    }

    /// Resolves an entry to a live URL. Refreshes the bookmark in place if `isStale`.
    /// Returned URL has NOT been start-accessed; caller must wrap in `SecurityScopedURL`.
    func resolve(_ entry: RecentEntry) -> Result<URL, ResolveError> {
        var stale = false
        do {
            let url = try URL(
                resolvingBookmarkData: entry.bookmark,
                options: [.withSecurityScope],
                relativeTo: nil,
                bookmarkDataIsStale: &stale
            )
            if stale {
                // Need to start access to recreate the bookmark.
                let scope = SecurityScopedURL(url: url)
                if scope.start(),
                   let fresh = try? url.bookmarkData(
                    options: [.withSecurityScope],
                    includingResourceValuesForKeys: nil,
                    relativeTo: nil
                   ) {
                    if let idx = entries.firstIndex(of: entry) {
                        entries[idx].bookmark = fresh
                        entries[idx].displayPath = url.path
                        persist()
                    }
                }
                scope.stop()
            }
            return .success(url)
        } catch {
            return .failure(.bookmarkInvalid(error))
        }
    }
}
```

- [ ] **Step 2: Build to verify the file compiles in isolation**

Run: `bash install-preview.sh`
Expected: build fails — `WelcomeView` and `LuraAppModel` still reference the old `urls`/`recordOpened` API in ways that need updating. Note the failures (referencing `recent.urls` is still valid because we kept it as a computed shim; the failures will mostly be in Task 5 once `RecentRow` switches to entries). If build still passes here, that's fine too — the shim keeps the old call sites working temporarily.

- [ ] **Step 3: If build fails, defer and continue — fixes land in Task 4 + Task 7**

The temporary `urls` shim should keep things compiling. If something else breaks (e.g. `RecentFilesStore` shape changed), revert to a stub by leaving `entries` empty until later tasks.

- [ ] **Step 4: Commit**

```bash
git add quicklook/HostApp/RecentFilesStore.swift
git commit -m "feat(host): RecentFilesStore stores security-scoped bookmarks (v2 schema, v1 migration)"
```

---

## Task 4: `LuraAppDelegate` — handle `application(_:open:)`

**Files:**

- Modify: `quicklook/HostApp/LuraAppDelegate.swift`

- [ ] **Step 1: Replace the delegate**

Overwrite `quicklook/HostApp/LuraAppDelegate.swift` with:

```swift
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
```

- [ ] **Step 2: Build and verify the file compiles**

Run: `bash install-preview.sh`
Expected: passes (the `model.register` callsite hasn't been added yet but compiler does not require it — `register(model:)` is dormant for now).

- [ ] **Step 3: Commit**

```bash
git add quicklook/HostApp/LuraAppDelegate.swift
git commit -m "feat(host): AppDelegate handles application(_:open:) with pending-URL queue"
```

---

## Task 5: `LuraApp` — wire `.onOpenURL` and register model with delegate

**Files:**

- Modify: `quicklook/HostApp/LuraApp.swift`

- [ ] **Step 1: Replace the file**

Overwrite `quicklook/HostApp/LuraApp.swift` with:

```swift
import SwiftUI

struct RootView: View {
    @EnvironmentObject private var appModel: LuraAppModel

    var body: some View {
        Group {
            if let url = appModel.openEditorURL {
                LuraEditorContainer(url: url, onClose: { appModel.openEditorURL = nil })
                    .id(url)
            } else {
                WelcomeView()
            }
        }
    }
}

@main
struct LuraApp: App {
    @NSApplicationDelegateAdaptor(LuraAppDelegate.self) private var appDelegate
    @StateObject private var appModel = LuraAppModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(appModel)
                .onOpenURL { url in
                    LuraDebugLog.log("WindowGroup.onOpenURL url=\(url.lastPathComponent)")
                    appModel.openDocumentURL(url)
                }
                .task {
                    appDelegate.register(model: appModel)
                }
        }
        .defaultSize(width: 960, height: 640)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Lura Document…") {
                    appModel.presentNewDocument()
                }
                .keyboardShortcut("n", modifiers: [.command])
            }
            CommandGroup(after: .newItem) {
                Button("Open…") {
                    appModel.presentOpenDocument()
                }
                .keyboardShortcut("o", modifiers: [.command])
            }
        }
    }
}
```

- [ ] **Step 2: Build**

Run: `bash install-preview.sh`
Expected: build passes.

- [ ] **Step 3: Commit**

```bash
git add quicklook/HostApp/LuraApp.swift
git commit -m "feat(host): wire WindowGroup.onOpenURL and register model with AppDelegate"
```

---

## Task 6: `LuraFileDocument` and `LuraAppModel` — track scope on open document

**Files:**

- Modify: `quicklook/HostApp/LuraFileDocument.swift`
- Modify: `quicklook/HostApp/LuraAppModel.swift`
- Modify: `quicklook/HostApp/LuraApp.swift` (one-line change in `RootView`)

- [ ] **Step 1: Update `LuraFileDocument` to hold a scope**

Overwrite `quicklook/HostApp/LuraFileDocument.swift` with:

```swift
import Foundation

@MainActor
final class LuraFileDocument: ObservableObject {
    let url: URL

    @Published var text: String

    /// True when buffer differs from last saved or reverted content.
    var isDirty: Bool { text != savedText }

    private var savedText: String

    /// Optional. Set when the URL was resolved from a security-scoped bookmark
    /// (Recents). When set, `deinit` calls `stop()`. Powerbox-granted URLs
    /// (Open / Save panel, Finder double-click) do NOT need this — system
    /// manages their scope.
    private var scope: SecurityScopedURL?

    init(url: URL, scope: SecurityScopedURL? = nil) throws {
        self.url = url
        self.scope = scope
        let loaded = try String(contentsOf: url, encoding: .utf8)
        self.savedText = loaded
        self.text = loaded
    }

    func save() throws {
        try text.write(to: url, atomically: true, encoding: .utf8)
        savedText = text
        objectWillChange.send()
    }

    func revert() throws {
        let loaded = try String(contentsOf: url, encoding: .utf8)
        savedText = loaded
        text = loaded
    }

    deinit {
        scope?.stop()
    }
}
```

- [ ] **Step 2: Update `LuraAppModel` — open by URL or by RecentEntry, plumb scope**

Open `quicklook/HostApp/LuraAppModel.swift`. Make the following four changes:

**Change 2a — Add stored property.** Just under the existing `@Published var editorIsDirty: Bool = false` line, insert:

```swift
    /// Held while a Recents-resolved document is open so its security-scoped
    /// access lives at least as long as the editor view. `nil` for Powerbox-
    /// granted URLs (Open / Save panel, Finder double-click) — system manages
    /// their scope.
    private var activeScope: SecurityScopedURL?

    /// Single point where `openEditorURL` is mutated. Always releases the old
    /// scope first; `newScope` is retained for the lifetime of the new editor.
    private func setOpenEditor(url: URL?, scope: SecurityScopedURL?) {
        activeScope?.stop()
        activeScope = scope
        openEditorURL = url
        editorIsDirty = false
    }
```

**Change 2b — Replace the existing `openDocumentURL(_:)` method** (currently around lines 144–149) with:

```swift
    func openDocumentURL(_ url: URL) {
        if !mayReplaceOpenDocument() { return }
        RecentFilesStore.shared.recordOpened(url)
        setOpenEditor(url: url, scope: nil)
    }

    func openRecent(_ entry: RecentEntry) {
        if !mayReplaceOpenDocument() { return }
        switch RecentFilesStore.shared.resolve(entry) {
        case .success(let url):
            let scope = SecurityScopedURL(url: url)
            guard scope.start() else {
                presentAlert(
                    title: "Could not open file",
                    message: "Sandbox refused access to \(url.lastPathComponent). It may have moved."
                )
                return
            }
            RecentFilesStore.shared.recordOpened(url)
            setOpenEditor(url: url, scope: scope)
        case .failure(let err):
            presentAlert(
                title: "Could not open recent file",
                message: "\(err)"
            )
            RecentFilesStore.shared.remove(entry)
        }
    }
```

**Change 2c — Route `finishOpen` and `finishNewDocument` through `setOpenEditor`.**

In `finishNewDocument(panel:response:)`, replace the success block:

```swift
            try LuraTemplates.newDocument.write(to: url, atomically: true, encoding: .utf8)
            LuraDebugLog.log("write template OK, opening editor")
            RecentFilesStore.shared.recordOpened(url)
            openEditorURL = url
            editorIsDirty = false
```

with:

```swift
            try LuraTemplates.newDocument.write(to: url, atomically: true, encoding: .utf8)
            LuraDebugLog.log("write template OK, opening editor")
            RecentFilesStore.shared.recordOpened(url)
            setOpenEditor(url: url, scope: nil)
```

In `finishOpen(panel:response:)`, replace the body:

```swift
        guard response == .OK, let url = panel.url else { return }
        RecentFilesStore.shared.recordOpened(url)
        openEditorURL = url
        editorIsDirty = false
```

with:

```swift
        guard response == .OK, let url = panel.url else { return }
        RecentFilesStore.shared.recordOpened(url)
        setOpenEditor(url: url, scope: nil)
```

**Change 2d — Cleanup on editor close.** The `RootView` calls `appModel.openEditorURL = nil` directly when the user clicks Close. Replace that pattern by exposing a `closeEditor()` method.

In `LuraAppModel`, add (place just below `setOpenEditor`):

```swift
    func closeEditor() {
        setOpenEditor(url: nil, scope: nil)
    }
```

In `quicklook/HostApp/LuraApp.swift`, locate the line:

```swift
                LuraEditorContainer(url: url, onClose: { appModel.openEditorURL = nil })
```

Replace with:

```swift
                LuraEditorContainer(url: url, onClose: { appModel.closeEditor() })
```

`LuraFileDocument(url:)` continues to load via `String(contentsOf: url)`. The model's `activeScope` keeps access alive for as long as the document is open.

- [ ] **Step 3: Build**

Run: `bash install-preview.sh`
Expected: build passes.

- [ ] **Step 4: Commit**

```bash
git add quicklook/HostApp/LuraFileDocument.swift quicklook/HostApp/LuraAppModel.swift quicklook/HostApp/LuraApp.swift
git commit -m "feat(host): track security-scoped access for Recents-opened documents"
```

---

## Task 7: `WelcomeView` — RecentRow consumes `RecentEntry`, handles stale

**Files:**

- Modify: `quicklook/HostApp/WelcomeView.swift`

- [ ] **Step 1: Update Recents section to use entries**

In `quicklook/HostApp/WelcomeView.swift`, locate the `if !recent.urls.isEmpty` block (around line 48). Replace it with:

```swift
            if !recent.entries.isEmpty {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text("Recent")
                            .font(.headline)
                        Spacer()
                        Button("Clear") {
                            recent.clearAll()
                        }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.secondary)
                        .font(.caption)
                    }
                    .padding(.horizontal, 4)

                    ScrollView {
                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(recent.entries, id: \.displayPath) { entry in
                                RecentRow(entry: entry) {
                                    appModel.openRecent(entry)
                                } onRemove: {
                                    recent.remove(entry)
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(maxHeight: 200)
                }
                .padding(.top, 36)
                .padding(.horizontal, 48)
            }
```

- [ ] **Step 2: Update `RecentRow` to accept `RecentEntry`**

In the same file, replace the existing `private struct RecentRow: View { ... }` block at the bottom with:

```swift
private struct RecentRow: View {
    let entry: RecentEntry
    let open: () -> Void
    let onRemove: () -> Void

    private let rowShape = RoundedRectangle(cornerRadius: 8, style: .continuous)

    private var displayURL: URL { URL(fileURLWithPath: entry.displayPath) }

    var body: some View {
        Button(action: open) {
            HStack {
                Image(systemName: "doc.plaintext")
                    .foregroundStyle(.secondary)
                    .frame(width: 20)
                VStack(alignment: .leading, spacing: 2) {
                    Text(displayURL.lastPathComponent)
                        .font(.body.weight(.medium))
                        .lineLimit(1)
                    Text(displayURL.deletingLastPathComponent().path)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.quaternary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                rowShape
                    .fill(Color.primary.opacity(0.04))
            )
            .contentShape(rowShape)
        }
        .buttonStyle(.borderless)
        .contextMenu {
            Button("Remove from Recents", role: .destructive, action: onRemove)
        }
    }
}
```

- [ ] **Step 3: Build**

Run: `bash install-preview.sh`
Expected: build passes.

- [ ] **Step 4: Commit**

```bash
git add quicklook/HostApp/WelcomeView.swift
git commit -m "feat(host): RecentRow uses RecentEntry; right-click to remove from Recents"
```

---

## Task 8: Manual smoke test pass

**Files:** none (verification only)

The first launch after this work runs the v1→v2 Recents migration. Old recents entries become bookmarks based on path; if a path no longer exists or sandbox denies bookmark creation, the entry is dropped — that is acceptable.

- [ ] **Step 1: Build and install**

Run: `bash install-preview.sh`
Expected: build succeeds, app installed to `~/Applications/Lura.app`.

- [ ] **Step 2: Cold-launch open**

```bash
killall Lura 2>/dev/null
open -a Lura ~/dev/personal/lura/examples/full-feature-test.pdf  # this should NOT open (wrong UTI), used to confirm no stale state
killall Lura 2>/dev/null
# create a quick test file
echo "# Hello Lura" > /tmp/smoke.lura
open /tmp/smoke.lura
```

Expected: Lura launches and lands directly on the editor for `/tmp/smoke.lura`. No Welcome screen flash longer than the SwiftUI scene attach.

Tail debug log:
`tail -n 30 "$HOME/Library/Containers/com.fallowlone.lura-document-app/Data/Library/Caches/LuraDebug/ui.log"`
Expected entries: `application(_:open:) urls=["smoke.lura"]`, `AppDelegate: draining 1 pending URLs`, no errors.

- [ ] **Step 3: Warm-launch open**

With Lura still running, in Finder double-click any other `.lura` file (or run `open /tmp/smoke2.lura` after `echo "# Two" > /tmp/smoke2.lura`).

Expected: editor switches to the new file. Debug log shows `WindowGroup.onOpenURL url=smoke2.lura` (and possibly also `application(_:open:)`).

- [ ] **Step 4: Recents persist across restart**

```bash
killall Lura
open -a Lura
```

Click `/tmp/smoke.lura` in the Recents list.

Expected: file opens without any permission prompt.

- [ ] **Step 5: Stale recents handling**

```bash
killall Lura
mv /tmp/smoke.lura /tmp/smoke-renamed.lura
open -a Lura
```

Click the renamed entry from Recents.

Expected: bookmark resolution updates `displayPath` silently and the file opens — OR an error alert appears with "Could not open recent file" and the entry is removed from the list.

- [ ] **Step 6: Right-click Remove from Recents**

Right-click any Recents entry → choose "Remove from Recents".

Expected: row disappears, persists after restart.

- [ ] **Step 7: Commit a brief verification note**

Create `docs/superpowers/plans/2026-04-17-phase-a-fileopen-verification.md` with bullet results of steps 2–6. Commit:

```bash
git add docs/superpowers/plans/2026-04-17-phase-a-fileopen-verification.md
git commit -m "docs: phase A smoke test verification log"
```

---

## Acceptance criteria (from spec, mirrored here)

- [ ] Double-click `.lura` from Finder (cold) opens directly in editor — no Welcome screen detour.
- [ ] Double-click `.lura` from Finder (warm) opens directly in editor.
- [ ] Recents click opens file with no permission prompt, including after `killall Lura` and reboot.
- [ ] Stale Recents entry is offered for removal, no crash.
