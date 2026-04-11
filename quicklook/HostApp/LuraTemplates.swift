import Foundation

enum LuraTemplates {
    static let newDocument = """
    STYLES({
      #accent: #3498DB
    })

    PAGE(
      H1({color: #accent} Untitled)
      P(Start writing your Lura document here.)
    )
    """
}
