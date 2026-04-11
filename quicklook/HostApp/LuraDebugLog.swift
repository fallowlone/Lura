import Foundation
import OSLog

/// Debug traces for the host app.
///
/// Terminal (live, Unified Logging):
///   log stream --style compact --info --predicate 'subsystem == "com.fallowlone.lura-document-app"'
///
/// Terminal (file in app sandbox):
///   tail -f "$HOME/Library/Containers/com.fallowlone.lura-document-app/Data/Library/Caches/LuraDebug/ui.log"
///
/// If you launch the binary from a shell, prints also go to stderr:
///   ~/Applications/Lura.app/Contents/MacOS/Lura
enum LuraDebugLog {
    static let subsystem = "com.fallowlone.lura-document-app"

    private static let logger = Logger(subsystem: subsystem, category: "HostUI")

    private static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    private static let fileQueue = DispatchQueue(label: "com.fallowlone.lura-document-app.debuglog")

    /// Writable inside the sandbox; nil only if Caches is unavailable.
    static var fileURL: URL? = {
        guard let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else {
            return nil
        }
        let dir = base.appendingPathComponent("LuraDebug", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            return dir.appendingPathComponent("ui.log")
        } catch {
            return nil
        }
    }()

    static func log(_ message: String, file: StaticString = #file, line u: UInt = #line) {
        let baseName = URL(fileURLWithPath: "\(file)").lastPathComponent
        let formatted = "[\(iso.string(from: Date()))] [main=\(Thread.isMainThread)] [\(baseName):\(u)] \(message)"
        logger.info("\(formatted, privacy: .public)")
        fputs("\(formatted)\n", stderr)
        fflush(stderr)
        appendToFile(formatted)
    }

    private static func appendToFile(_ formatted: String) {
        guard let url = fileURL else { return }
        let data = (formatted + "\n").data(using: .utf8) ?? Data()
        fileQueue.async {
            if !FileManager.default.fileExists(atPath: url.path) {
                FileManager.default.createFile(atPath: url.path, contents: nil, attributes: nil)
            }
            guard let handle = try? FileHandle(forWritingTo: url) else { return }
            defer { try? handle.close() }
            _ = try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
        }
    }
}
