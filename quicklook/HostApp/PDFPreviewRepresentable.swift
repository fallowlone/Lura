import AppKit
import PDFKit
import SwiftUI

struct PDFPreviewRepresentable: NSViewRepresentable {
    var pdfData: Data?
    var matches: [LuraTextUnit] = []
    var currentMatchIndex: Int = 0

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

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
        context.coordinator.pdfView = view
        return view
    }

    func updateNSView(_ pdfView: PDFView, context: Context) {
        let coord = context.coordinator

        // The deferred block below runs after later updates may have arrived, so it reads the
        // newest highlight state from the coordinator, never from this struct's captured copy.
        coord.matches = matches
        coord.currentMatchIndex = currentMatchIndex

        // Reload the PDFDocument when the bytes actually changed (full comparison: a hash
        // collision would otherwise keep showing the old document).
        let dataChanged = coord.lastData != pdfData
        if dataChanged {
            coord.lastData = pdfData
            if let data = pdfData, !data.isEmpty, let doc = PDFDocument(data: data) {
                pdfView.document = doc
                DispatchQueue.main.async { [weak coord, weak pdfView] in
                    guard let coord, let pdfView else { return }
                    self.scrollToTop(pdfView)
                    self.applyHighlights(pdfView, coord: coord)
                }
                return
            } else {
                pdfView.document = nil
                coord.currentHighlights.removeAll()
                return
            }
        }

        // Doc unchanged — only refresh highlights.
        applyHighlights(pdfView, coord: coord)
    }

    // MARK: - Highlights

    private func applyHighlights(_ pdfView: PDFView, coord: Coordinator) {
        guard let doc = pdfView.document else { return }

        // Remove previous annotations.
        for (page, annot) in coord.currentHighlights {
            page.removeAnnotation(annot)
        }
        coord.currentHighlights.removeAll()

        let matches = coord.matches
        let currentMatchIndex = coord.currentMatchIndex
        guard !matches.isEmpty else { return }

        for (idx, unit) in matches.enumerated() {
            guard unit.page < doc.pageCount, let page = doc.page(at: unit.page) else { continue }
            let rect = NSRect(x: unit.x, y: unit.y, width: unit.w, height: unit.h)
            let annot = PDFAnnotation(bounds: rect, forType: .highlight, withProperties: nil)
            annot.color = (idx == currentMatchIndex)
                ? NSColor.systemOrange.withAlphaComponent(0.55)
                : NSColor.systemYellow.withAlphaComponent(0.40)
            page.addAnnotation(annot)
            coord.currentHighlights.append((page, annot))
        }

        // Scroll current match into view.
        guard currentMatchIndex < matches.count else { return }
        let cur = matches[currentMatchIndex]
        guard cur.page < doc.pageCount, let page = doc.page(at: cur.page) else { return }
        let rect = NSRect(x: cur.x, y: cur.y, width: cur.w, height: cur.h)
        pdfView.go(to: PDFDestination(page: page, at: NSPoint(x: rect.minX, y: rect.maxY)))
    }

    private func scrollToTop(_ pdfView: PDFView) {
        guard let scrollView = pdfView.enclosingScrollView else { return }
        let clipView = scrollView.contentView

        pdfView.layoutDocumentView()

        guard let docView = clipView.documentView else { return }
        let docFrame = docView.frame
        let clipBounds = clipView.bounds
        let maxScrollY = NSMaxY(docFrame) - NSHeight(clipBounds)
        let targetPoint = NSPoint(x: 0, y: maxScrollY)

        clipView.scroll(to: targetPoint)
        scrollView.reflectScrolledClipView(clipView)
    }

    final class Coordinator {
        weak var pdfView: PDFView?
        var lastData: Data?
        var matches: [LuraTextUnit] = []
        var currentMatchIndex = 0
        var currentHighlights: [(PDFPage, PDFAnnotation)] = []
    }
}
