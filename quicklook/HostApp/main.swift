import SwiftUI
import AppKit
import UniformTypeIdentifiers

// MARK: - Document template

private let newDocumentTemplate = """
STYLES({
  #accent: #3498DB
})

PAGE(
  H1({color: #accent} Untitled)
  P(Start writing your Lura document here.)
)
"""

// MARK: - Recent files

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

// MARK: - File panels

@MainActor
enum LuraPanels {
    /// `.lura` preferred; `.fol` kept for existing files.
    private static var documentTypes: [UTType] {
        [UTType(filenameExtension: "lura"), UTType(filenameExtension: "fol")]
            .compactMap { $0 }
    }

    static func presentNewDocument() {
        let panel = NSSavePanel()
        panel.title = "New Lura document"
        panel.nameFieldStringValue = "Untitled.lura"
        panel.allowedContentTypes = documentTypes
        panel.canCreateDirectories = true
        panel.isExtensionHidden = false

        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try newDocumentTemplate.write(to: url, atomically: true, encoding: .utf8)
            RecentFilesStore.shared.recordOpened(url)
            NSWorkspace.shared.open(url)
        } catch {
            presentAlert(title: "Could not create file", message: error.localizedDescription)
        }
    }

    static func presentOpenDocument() {
        let panel = NSOpenPanel()
        panel.title = "Open Lura document"
        panel.allowedContentTypes = documentTypes
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.canChooseFiles = true

        guard panel.runModal() == .OK, let url = panel.url else { return }
        RecentFilesStore.shared.recordOpened(url)
        NSWorkspace.shared.open(url)
    }

    private static func presentAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.runModal()
    }
}

// MARK: - Welcome UI

struct WelcomeView: View {
    @ObservedObject private var recent = RecentFilesStore.shared

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(nsColor: .windowBackgroundColor),
                    Color(nsColor: .controlBackgroundColor).opacity(0.95),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            VStack(spacing: 0) {
                Spacer(minLength: 28)

                VStack(spacing: 10) {
                    Image(systemName: "doc.text.fill")
                        .font(.system(size: 52, weight: .medium))
                        .symbolRenderingMode(.hierarchical)
                        .foregroundStyle(.tint)

                    Text("Lura")
                        .font(.system(size: 34, weight: .bold, design: .rounded))
                    Text("Documents and Quick Look preview")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                .padding(.bottom, 36)

                HStack(spacing: 20) {
                    ActionCard(
                        systemImage: "plus.circle.fill",
                        title: "New document",
                        subtitle: "Create a .lura file",
                        accent: .accentColor
                    ) {
                        LuraPanels.presentNewDocument()
                    }

                    ActionCard(
                        systemImage: "folder.fill",
                        title: "Open…",
                        subtitle: "Browse for a file",
                        accent: .secondary
                    ) {
                        LuraPanels.presentOpenDocument()
                    }
                }
                .padding(.horizontal, 40)

                if !recent.urls.isEmpty {
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
                                ForEach(recent.urls, id: \.path) { url in
                                    RecentRow(url: url) {
                                        RecentFilesStore.shared.recordOpened(url)
                                        NSWorkspace.shared.open(url)
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

                Spacer(minLength: 24)

                Text("Editor and visual tools will land here. Scripting (loops, patterns) is planned in the format, Typst-style.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 56)
                    .padding(.bottom, 20)
            }
            .frame(maxWidth: 560)
        }
        .frame(minWidth: 640, minHeight: 460)
    }
}

private struct ActionCard: View {
    let systemImage: String
    let title: String
    let subtitle: String
    let accent: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 12) {
                Image(systemName: systemImage)
                    .font(.system(size: 28))
                    .foregroundStyle(accent)
                Text(title)
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.primary)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, minHeight: 120, alignment: .leading)
            .padding(20)
            .background(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .onHover { inside in
            if inside {
                NSCursor.pointingHand.push()
            } else {
                NSCursor.pop()
            }
        }
    }
}

private struct RecentRow: View {
    let url: URL
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack {
                Image(systemName: "doc.plaintext")
                    .foregroundStyle(.secondary)
                    .frame(width: 20)
                VStack(alignment: .leading, spacing: 2) {
                    Text(url.lastPathComponent)
                        .font(.body.weight(.medium))
                        .lineLimit(1)
                    Text(url.deletingLastPathComponent().path)
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
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color.primary.opacity(0.04))
            )
        }
        .buttonStyle(.plain)
    }
}

// MARK: - App

@main
struct LuraApp: App {
    var body: some Scene {
        WindowGroup {
            WelcomeView()
        }
        .defaultSize(width: 720, height: 520)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Lura Document…") {
                    LuraPanels.presentNewDocument()
                }
                .keyboardShortcut("n", modifiers: [.command])
            }
            CommandGroup(after: .newItem) {
                Button("Open…") {
                    LuraPanels.presentOpenDocument()
                }
                .keyboardShortcut("o", modifiers: [.command])
            }
        }
    }
}
