import Foundation

/// Optional `basename.fol.preview.pdf` next to the source for instant cold Quick Look.
enum LuraPreviewSidecar {
    static func url(for documentURL: URL) -> URL {
        documentURL.deletingLastPathComponent()
            .appendingPathComponent(documentURL.lastPathComponent + ".preview.pdf", isDirectory: false)
    }

    /// PDF bytes if sidecar exists, is valid PDF, and is not older than the source file.
    static func pdfIfFresh(documentURL: URL) -> Data? {
        let side = url(for: documentURL)
        guard FileManager.default.fileExists(atPath: side.path) else { return nil }
        let srcVals = try? documentURL.resourceValues(forKeys: [.contentModificationDateKey])
        let sideVals = try? side.resourceValues(forKeys: [.contentModificationDateKey])
        guard let srcDate = srcVals?.contentModificationDate,
              let sideDate = sideVals?.contentModificationDate,
              sideDate >= srcDate else {
            return nil
        }
        guard let data = try? Data(contentsOf: side), data.count >= 5 else { return nil }
        guard data.starts(with: Data([0x25, 0x50, 0x44, 0x46])) else { return nil }
        return data
    }

    static func write(pdf: Data, documentURL: URL) {
        guard pdf.count >= 5, pdf.starts(with: Data([0x25, 0x50, 0x44, 0x46])) else { return }
        let dest = url(for: documentURL)
        do {
            try pdf.write(to: dest, options: .atomic)
        } catch {
            // Sandbox or read-only volume; preview still works without sidecar.
        }
    }
}
