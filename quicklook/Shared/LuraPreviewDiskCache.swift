import CryptoKit
import Foundation

/// Persists rendered PDF bytes keyed by SHA256 of the source document.
/// Quick Look often cold-starts; without this every preview pays full parse → layout → PDF.
///
/// Uses App Group container when available so the Lura editor can prewarm the same cache as the
/// Quick Look extension (`install-preview.sh` adds `com.apple.security.application-groups`).
enum LuraPreviewDiskCache {
    /// Must match entitlements on the host app and the QL extension.
    private static let appGroupIdentifier = "group.com.fallowlone.lura"

    private static let subdirectory = "LuraPreviewRender"
    private static let maxEntries = 96

    static func pdf(forDocumentData data: Data) -> Data? {
        let name = sha256Hex(data) + ".pdf"
        guard let dir = try? cacheDirectory() else { return nil }
        let url = dir.appendingPathComponent(name)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        guard let pdf = try? Data(contentsOf: url), pdf.count >= 5 else { return nil }
        // Reject corrupted cache entries (PDF magic).
        guard pdf.starts(with: Data([0x25, 0x50, 0x44, 0x46])) else { // %PDF
            try? FileManager.default.removeItem(at: url)
            return nil
        }
        return pdf
    }

    static func store(_ pdf: Data, forDocumentData data: Data) {
        guard pdf.count >= 5, pdf.starts(with: Data([0x25, 0x50, 0x44, 0x46])) else { return }
        guard let dir = try? cacheDirectory() else { return }
        let name = sha256Hex(data) + ".pdf"
        let url = dir.appendingPathComponent(name)
        do {
            try pdf.write(to: url, options: .atomic)
            trimIfNeeded(in: dir)
        } catch {
            // Best-effort cache; preview still succeeded in memory.
        }
    }

    private static func cacheDirectory() throws -> URL {
        if let shared = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroupIdentifier) {
            let dir = shared
                .appendingPathComponent("Library", isDirectory: true)
                .appendingPathComponent("Caches", isDirectory: true)
                .appendingPathComponent(subdirectory, isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            return dir
        }
        let base = try FileManager.default.url(
            for: .cachesDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dir = base.appendingPathComponent(subdirectory, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private static func sha256Hex(_ data: Data) -> String {
        let digest = SHA256.hash(data: data)
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    private static func trimIfNeeded(in dir: URL) {
        guard let files = try? FileManager.default.contentsOfDirectory(
            at: dir,
            includingPropertiesForKeys: [.contentModificationDateKey],
            options: [.skipsHiddenFiles]
        ) else { return }

        let pdfs = files.filter { $0.pathExtension.lowercased() == "pdf" }
        guard pdfs.count > maxEntries else { return }

        let dated: [(url: URL, date: Date)] = pdfs.compactMap { url in
            let d = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
                ?? .distantPast
            return (url, d)
        }
        let sorted = dated.sorted { $0.date < $1.date }
        let removeCount = pdfs.count - maxEntries
        for i in 0..<removeCount {
            try? FileManager.default.removeItem(at: sorted[i].url)
        }
    }
}
