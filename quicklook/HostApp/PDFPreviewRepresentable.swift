import AppKit
import PDFKit
import SwiftUI

struct PDFPreviewRepresentable: NSViewRepresentable {
    var pdfData: Data?

    func makeNSView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        view.displayMode = .singlePageContinuous
        view.displayDirection = .vertical
        view.pageShadowsEnabled = true
        view.backgroundColor = NSColor.controlBackgroundColor
        if #available(macOS 11.0, *) {
            view.pageBreakMargins = NSEdgeInsets(top: 6, left: 4, bottom: 6, right: 4)
        }
        return view
    }

    func updateNSView(_ pdfView: PDFView, context: Context) {
        if let data = pdfData, !data.isEmpty, let doc = PDFDocument(data: data) {
            pdfView.document = doc
            // Defer scrolling until after the view has been laid out
            DispatchQueue.main.async {
                self.scrollToTop(pdfView)
            }
        } else {
            pdfView.document = nil
        }
    }

    private func scrollToTop(_ pdfView: PDFView) {
        guard let scrollView = pdfView.enclosingScrollView else { return }
        let clipView = scrollView.contentView

        pdfView.layoutDocumentView()

        // PDFKit origin is bottom-left; scroll to maxY to reach the top
        guard let docView = clipView.documentView else { return }
        let docFrame = docView.frame
        let clipBounds = clipView.bounds
        let maxScrollY = NSMaxY(docFrame) - NSHeight(clipBounds)
        let targetPoint = NSPoint(x: 0, y: maxScrollY)

        clipView.scroll(to: targetPoint)
        scrollView.reflectScrolledClipView(clipView)
    }
}
