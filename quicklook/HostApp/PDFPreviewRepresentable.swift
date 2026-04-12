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
            pdfView.layoutDocumentView()
            pdfView.goToFirstPage(nil)
            DispatchQueue.main.async {
                pdfView.layoutDocumentView()
                pdfView.goToFirstPage(nil)
            }
        } else {
            pdfView.document = nil
        }
    }
}
