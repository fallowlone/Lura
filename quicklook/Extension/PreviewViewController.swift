import Cocoa
import PDFKit
import Quartz

@objc(PreviewViewController)
class PreviewViewController: NSViewController, QLPreviewingController {

    private let pdfView = PDFView()
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
        pdfView.autoScales = true
        pdfView.displayMode = .singlePageContinuous
        pdfView.displayDirection = .vertical
        container.addSubview(pdfView)
        container.addSubview(errorLabel)
        NSLayoutConstraint.activate([
            pdfView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            pdfView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            pdfView.topAnchor.constraint(equalTo: container.topAnchor),
            pdfView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            errorLabel.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 16),
            errorLabel.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -16),
            errorLabel.topAnchor.constraint(equalTo: container.topAnchor, constant: 16),
        ])
        self.view = container
    }

    private func showError(_ message: String) {
        errorLabel.stringValue = message
        errorLabel.isHidden = false
        pdfView.document = nil
    }

    private func showPDF(data: Data) {
        errorLabel.isHidden = true
        pdfView.document = PDFDocument(data: data)
    }

    func preparePreviewOfFile(at url: URL, completionHandler handler: @escaping (Error?) -> Void) {
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

        do {
            let fileData = try Data(contentsOf: url)
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
                showPDF(data: data)
            } else {
                showError("No PDF data returned.")
            }
            handler(nil)
        } catch {
            showError("File read error: \(error.localizedDescription)")
            handler(nil)
        }
    }
}
