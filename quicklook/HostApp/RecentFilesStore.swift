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
                do {
                    let bm = try url.bookmarkData(
                        options: [.withSecurityScope],
                        includingResourceValuesForKeys: nil,
                        relativeTo: nil
                    )
                    entries.append(RecentEntry(bookmark: bm, displayPath: url.path, lastOpened: Date()))
                } catch {
                    LuraDebugLog.log("RecentFilesStore.migration: drop \(path) — bookmarkData failed \(error.localizedDescription)")
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
                if scope.start() {
                    do {
                        let fresh = try url.bookmarkData(
                            options: [.withSecurityScope],
                            includingResourceValuesForKeys: nil,
                            relativeTo: nil
                        )
                        if let idx = entries.firstIndex(of: entry) {
                            entries[idx].bookmark = fresh
                            entries[idx].displayPath = url.path
                            persist()
                        }
                    } catch {
                        LuraDebugLog.log("RecentFilesStore.resolve: stale bookmark refresh failed \(error.localizedDescription)")
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
