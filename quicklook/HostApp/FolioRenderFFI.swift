import Darwin
import Foundation

/// Loads `libfolio.dylib` from the host app bundle once and calls `folio_render_html` on the main thread.
enum FolioRenderFFI {
    private static var handle: UnsafeMutableRawPointer?
    private static var symRender: UnsafeMutableRawPointer?
    private static var symFree: UnsafeMutableRawPointer?

    private static func loadLibrary() -> String? {
        if handle != nil { return nil }

        guard let fwPath = Bundle.main.privateFrameworksPath else {
            return "Bundle has no Frameworks path (expected Contents/Frameworks with libfolio.dylib)."
        }
        let path = (fwPath as NSString).appendingPathComponent("libfolio.dylib")
        guard let h = dlopen(path, RTLD_NOW) else {
            return String(cString: dlerror())
        }
        handle = h
        symRender = dlsym(h, "folio_render_html")
        symFree = dlsym(h, "folio_free_string")
        if symRender == nil || symFree == nil {
            return "Missing folio_render_html or folio_free_string in libfolio.dylib."
        }
        return nil
    }

    static func renderHTML(source: String) -> String {
        if let err = loadLibrary() {
            return errorPage(title: "Rust library", body: err)
        }

        typealias RenderFunc = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
        typealias FreeFunc = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
        let render = unsafeBitCast(symRender!, to: RenderFunc.self)
        let freeStr = unsafeBitCast(symFree!, to: FreeFunc.self)

        return source.withCString { cstr in
            guard let ptr = render(cstr) else {
                return errorPage(title: "Render Error", body: "Library returned null.")
            }
            defer { freeStr(ptr) }
            return String(cString: ptr)
        }
    }

    private static func errorPage(title: String, body: String) -> String {
        let safe = escapeHtml(body)
        return """
        <!DOCTYPE html>
        <html><head><meta charset="utf-8"><title>\(escapeHtml(title))</title></head>
        <body style="font-family: system-ui; padding: 1rem;"><h1>\(escapeHtml(title))</h1><pre style="white-space: pre-wrap;">\(safe)</pre></body></html>
        """
    }

    private static func escapeHtml(_ s: String) -> String {
        s
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
    }
}
