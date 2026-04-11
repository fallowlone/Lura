import Darwin
import Foundation

/// Loads `liblura.dylib` from the host app bundle once and calls `lura_render_pdf` on the main thread.
enum LuraRenderFFI {
    private static var handle: UnsafeMutableRawPointer?
    private static var symRender: UnsafeMutableRawPointer?
    private static var symFree: UnsafeMutableRawPointer?

    private static func loadLibrary() -> String? {
        if handle != nil { return nil }

        guard let fwPath = Bundle.main.privateFrameworksPath else {
            return "Bundle has no Frameworks path (expected Contents/Frameworks with liblura.dylib)."
        }
        let path = (fwPath as NSString).appendingPathComponent("liblura.dylib")
        guard let h = dlopen(path, RTLD_NOW) else {
            return String(cString: dlerror())
        }
        handle = h
        symRender = dlsym(h, "lura_render_pdf")
        symFree = dlsym(h, "lura_free_pdf_result")
        if symRender == nil || symFree == nil {
            return "Missing lura_render_pdf or lura_free_pdf_result in liblura.dylib."
        }
        return nil
    }

    static func renderPDF(source: String) -> LuraPdfFFI.Output {
        if let err = loadLibrary() {
            return LuraPdfFFI.Output(pdfData: nil, errorMessage: err)
        }
        return LuraPdfFFI.invokeRender(
            source: source,
            symRender: symRender!,
            symFree: symFree!
        )
    }
}
