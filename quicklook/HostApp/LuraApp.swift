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
