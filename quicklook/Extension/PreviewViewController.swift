import Cocoa
import PDFKit
import Quartz

@objc(PreviewViewController)
class PreviewViewController: NSViewController, QLPreviewingController {

    /// Last file URL passed to `preparePreviewOfFile` (for debug logging).
    private var lastPreviewDocumentURL: URL?

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

        // Use autoScales for optimized rendering pipeline
        pdfView.autoScales = true
        pdfView.displayMode = .singlePage
        pdfView.displayDirection = .vertical
        pdfView.pageShadowsEnabled = true
        pdfView.backgroundColor = NSColor.windowBackgroundColor

        thumbnailView.pdfView = pdfView
        thumbnailView.maximumNumberOfColumns = 1
        thumbnailView.allowsMultipleSelection = false
        thumbnailView.thumbnailSize = NSSize(width: 72, height: 96)
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

    /// Store PDF data and set first page immediately.
    private func showPDF(data: Data) {
        errorLabel.isHidden = true
        thumbnailView.isHidden = false
        // #region agent log
        let pcInit = PDFDocument(data: data)?.pageCount ?? -1
        LuraAgentSessionLog.append(
            hypothesisId: "H1",
            location: "PreviewViewController.showPDF",
            message: "pdf_stored",
            data: ["pdfBytes": data.count, "pageCountFromDoc": pcInit],
            siblingToDocument: lastPreviewDocumentURL
        )
        // #endregion

        DispatchQueue.main.async { [weak self] in
            guard let self = self, let doc = PDFDocument(data: data) else { return }

            self.pdfView.document = doc
            // .singlePage mode makes goToFirstPage reliable — no scroll race with
            // continuous-strip layout landing the view on the last page.
            self.pdfView.goToFirstPage(nil)

            // #region agent log
            let idx = self.pdfView.currentPage.map { doc.index(for: $0) } ?? -1
            LuraAgentSessionLog.append(
                hypothesisId: "H4",
                location: "PreviewViewController.showPDF",
                message: "document_set_after_handler",
                data: ["currentPageIndex": idx],
                siblingToDocument: self.lastPreviewDocumentURL
            )
            // #endregion
        }
    }

    func preparePreviewOfFile(at url: URL, completionHandler handler: @escaping (Error?) -> Void) {
        lastPreviewDocumentURL = url
        // #region agent log
        LuraAgentSessionLog.append(
            hypothesisId: "H1",
            location: "PreviewViewController.preparePreviewOfFile",
            message: "entry",
            data: ["file": url.lastPathComponent],
            siblingToDocument: url
        )
        // #endregion
        // Reading a sibling `.preview.pdf` from the Quick Look extension triggers the
        // macOS 15 "data from other apps" TCC prompt. The App Group disk cache below
        // already covers the warm path, so the sidecar fast-path is dropped.

        let fileData: Data
        do {
            fileData = try Data(contentsOf: url)
        } catch {
            showError("File read error: \(error.localizedDescription)")
            handler(nil)
            return
        }

        if let cached = LuraPreviewDiskCache.pdf(forDocumentData: fileData) {
            // #region agent log
            LuraAgentSessionLog.append(
                hypothesisId: "H2",
                location: "PreviewViewController.preparePreviewOfFile",
                message: "branch_disk_cache",
                data: ["bytes": cached.count, "srcFileBytes": fileData.count],
                siblingToDocument: url
            )
            // #endregion
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

        // #region agent log
        LuraAgentSessionLog.append(
            hypothesisId: "H2",
            location: "PreviewViewController.preparePreviewOfFile",
            message: "branch_ffi_dlopen",
            data: ["srcChars": contentStr.count],
            siblingToDocument: url
        )
        // #endregion
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
