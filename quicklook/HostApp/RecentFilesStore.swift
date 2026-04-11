import Foundation
import SwiftUI

@MainActor
final class RecentFilesStore: ObservableObject {
    static let shared = RecentFilesStore()

    @Published private(set) var urls: [URL] = []

    private let defaultsKey = "com.fallowlone.lura-document-app.recentFiles"
    private let maxEntries = 12

    private init() {
        refreshFromDisk()
    }

    func refreshFromDisk() {
        let paths = UserDefaults.standard.stringArray(forKey: defaultsKey) ?? []
        urls = paths.compactMap { path -> URL? in
            let u = URL(fileURLWithPath: path)
            return FileManager.default.fileExists(atPath: u.path) ? u : nil
        }
    }

    func recordOpened(_ url: URL) {
        var paths = urls.map(\.path)
        paths.removeAll { $0 == url.path }
        paths.insert(url.path, at: 0)
        if paths.count > maxEntries {
            paths = Array(paths.prefix(maxEntries))
        }
        UserDefaults.standard.set(paths, forKey: defaultsKey)
        refreshFromDisk()
    }

    func clearAll() {
        UserDefaults.standard.removeObject(forKey: defaultsKey)
        refreshFromDisk()
    }
}
