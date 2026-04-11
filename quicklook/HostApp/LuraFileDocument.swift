import Foundation

@MainActor
final class LuraFileDocument: ObservableObject {
    let url: URL

    @Published var text: String

    /// True when buffer differs from last saved or reverted content.
    var isDirty: Bool { text != savedText }

    private var savedText: String

    init(url: URL) throws {
        self.url = url
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
}
