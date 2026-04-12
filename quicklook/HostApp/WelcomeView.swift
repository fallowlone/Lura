import SwiftUI
import AppKit

struct WelcomeView: View {
    @EnvironmentObject private var appModel: LuraAppModel
    @ObservedObject private var recent = RecentFilesStore.shared

    var body: some View {
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
                    LuraDebugLog.log("WelcomeView: New document button action invoked")
                    appModel.presentNewDocument()
                }

                ActionCard(
                    systemImage: "folder.fill",
                    title: "Open…",
                    subtitle: "Browse for a file",
                    accent: .secondary
                ) {
                    appModel.presentOpenDocument()
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
                                    appModel.openDocumentURL(url)
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

            Text("Source editor with live PDF preview. Full visual editing is planned for a later release.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 56)
                .padding(.bottom, 20)
        }
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        .frame(minWidth: 640, minHeight: 460)
        .background {
            LinearGradient(
                colors: [
                    Color(nsColor: .windowBackgroundColor),
                    Color(nsColor: .controlBackgroundColor).opacity(0.95),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()
        }
    }
}

private struct ActionCard: View {
    let systemImage: String
    let title: String
    let subtitle: String
    let accent: Color
    let action: () -> Void

    private let cardShape = RoundedRectangle(cornerRadius: 14, style: .continuous)

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
                cardShape
                    .fill(Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                cardShape
                    .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
            )
            .contentShape(cardShape)
        }
        .buttonStyle(.borderless)
        .onHover { inside in
            if inside {
                NSCursor.pointingHand.set()
            } else {
                NSCursor.arrow.set()
            }
        }
    }
}

private struct RecentRow: View {
    let url: URL
    let open: () -> Void

    private let rowShape = RoundedRectangle(cornerRadius: 8, style: .continuous)

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
                rowShape
                    .fill(Color.primary.opacity(0.04))
            )
            .contentShape(rowShape)
        }
        .buttonStyle(.borderless)
    }
}
