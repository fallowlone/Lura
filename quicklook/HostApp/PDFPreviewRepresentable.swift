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
        view.backgroundColor = NSColor.controlBackgroundColor
        return view
    }

    func updateNSView(_ pdfView: PDFView, context: Context) {
        if let data = pdfData, !data.isEmpty, let doc = PDFDocument(data: data) {
            pdfView.document = doc
            DispatchQueue.main.async {
                pdfView.goToFirstPage(nil)
            }
        } else {
            pdfView.document = nil
        }
    }
}
