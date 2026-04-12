import Cocoa
import PDFKit
import Quartz

@objc(PreviewViewController)
class PreviewViewController: NSViewController, QLPreviewingController {

    private let pdfView = PDFView()
    private let thumbnailView = PDFThumbnailView()
    private let errorLabel: NSTextField = {
        let t = NSTextField(wrappingLabelWithString: "")
        t.textColor = .labelColor
        t.isEditable = false
        t.isHidden = true
        t.translatesAutoresizingMaskIntoConstraints = false
        return t
    }()

    override func loadView() {
        let container = NSView()

        pdfView.translatesAutoresizingMaskIntoConstraints = false
        thumbnailView.translatesAutoresizingMaskIntoConstraints = false

        pdfView.autoScales = true
        pdfView.displayMode = .singlePageContinuous
        pdfView.displayDirection = .vertical
        pdfView.pageShadowsEnabled = true
        pdfView.backgroundColor = NSColor.windowBackgroundColor
        if #available(macOS 11.0, *) {
            pdfView.pageBreakMargins = NSEdgeInsets(top: 8, left: 6, bottom: 8, right: 6)
        }

        thumbnailView.pdfView = pdfView
        thumbnailView.maximumNumberOfColumns = 1
        thumbnailView.allowsMultipleSelection = false
        thumbnailView.thumbnailSize = NSSize(width: 88, height: 118)
        thumbnailView.backgroundColor = NSColor.controlBackgroundColor

        container.addSubview(pdfView)
        container.addSubview(thumbnailView)
        container.addSubview(errorLabel)

        NSLayoutConstraint.activate([
            pdfView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            pdfView.topAnchor.constraint(equalTo: container.topAnchor),
            pdfView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            pdfView.trailingAnchor.constraint(equalTo: thumbnailView.leadingAnchor),

            thumbnailView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            thumbnailView.topAnchor.constraint(equalTo: container.topAnchor),
            thumbnailView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            thumbnailView.widthAnchor.constraint(equalToConstant: 104),

            errorLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 16),
            errorLabel.trailingAnchor.constraint(equalTo: thumbnailView.leadingAnchor, constant: -16),
            errorLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 16),
        ])
        self.view = container
    }

    private func showError(_ message: String) {
        errorLabel.stringValue = message
        errorLabel.isHidden = false
        pdfView.document = nil
        thumbnailView.isHidden = true
    }

    private func showPDF(data: Data) {
        errorLabel.isHidden = true
        thumbnailView.isHidden = false
        pdfView.document = PDFDocument(data: data)
        DispatchQueue.main.async { [weak self] in
            self?.scrollPDFToStart()
        }
    }

    /// Let PDFKit lay out the continuous strip, then jump to page 1. No manual NSScrollView bounds
    /// (that broke document coordinates). A slightly delayed pass catches late layout.
    private func scrollPDFToStart() {
        func snap() {
            pdfView.layoutDocumentView()
            pdfView.goToFirstPage(nil)
        }
        snap()
        DispatchQueue.main.async { [weak self] in
            self?.pdfView.layoutDocumentView()
            self?.pdfView.goToFirstPage(nil)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.06) { [weak self] in
            self?.pdfView.layoutDocumentView()
            self?.pdfView.goToFirstPage(nil)
        }
    }

    func preparePreviewOfFile(at url: URL, completionHandler handler: @escaping (Error?) -> Void) {
        if let sidecar = LuraPreviewSidecar.pdfIfFresh(documentURL: url) {
            showPDF(data: sidecar)
            handler(nil)
            return
        }

        let fileData: Data
        do {
            fileData = try Data(contentsOf: url)
        } catch {
            showError("File read error: \(error.localizedDescription)")
            handler(nil)
            return
        }

        if let cached = LuraPreviewDiskCache.pdf(forDocumentData: fileData) {
            showPDF(data: cached)
            handler(nil)
            return
        }

        let bundle = Bundle(for: type(of: self))
        guard let frameworksPath = bundle.privateFrameworksPath else {
            showError("Missing Frameworks path (expected liblura.dylib in the extension bundle).")
            handler(nil)
            return
        }
        let dylibPath = (frameworksPath as NSString).appendingPathComponent("liblura.dylib")

        guard let handle = dlopen(dylibPath, RTLD_NOW) else {
            let errStr = String(cString: dlerror())
            showError("dlopen failed: \(errStr)")
            handler(nil)
            return
        }

        defer { dlclose(handle) }

        guard let symRender = dlsym(handle, "lura_render_pdf"),
              let symFree = dlsym(handle, "lura_free_pdf_result") else {
            showError("dlsym: lura_render_pdf / lura_free_pdf_result not found in liblura.dylib.")
            handler(nil)
            return
        }

        guard let contentStr = String(data: fileData, encoding: .utf8) else {
            showError("The file is not valid UTF-8 text.")
            handler(nil)
            return
        }

        let out = LuraPdfFFI.invokeRender(
            source: contentStr,
            symRender: symRender,
            symFree: symFree
        )
        if let err = out.errorMessage {
            showError(err)
        } else if let data = out.pdfData {
            LuraPreviewDiskCache.store(data, forDocumentData: fileData)
            showPDF(data: data)
        } else {
            showError("No PDF data returned.")
        }
        handler(nil)
    }
}
