import Cocoa
import Quartz
import WebKit

@objc(PreviewViewController)
class PreviewViewController: NSViewController, QLPreviewingController {
    
    var webView: WKWebView!

    override func loadView() {
        self.webView = WKWebView()
        self.view = self.webView
    }

    func preparePreviewOfFile(at url: URL, completionHandler handler: @escaping (Error?) -> Void) {
        let bundle = Bundle(for: type(of: self))
        guard let frameworksPath = bundle.privateFrameworksPath else {
            let html = "<h1>Missing Frameworks Path</h1>"
            webView.loadHTMLString(html, baseURL: nil)
            handler(nil)
            return
        }
        let dylibPath = (frameworksPath as NSString).appendingPathComponent("libfolio.dylib")
        
        guard let handle = dlopen(dylibPath, RTLD_NOW) else {
            let errStr = String(cString: dlerror())
            let html = "<h1>dlopen Error</h1><pre>\(errStr)</pre>"
            webView.loadHTMLString(html, baseURL: nil)
            handler(nil)
            return
        }
        
        defer { dlclose(handle) }
        
        guard let symRender = dlsym(handle, "folio_render_html"),
              let symFree = dlsym(handle, "folio_free_string") else {
            let html = "<h1>dlsym Error</h1><p>Functions not found in libfolio.dylib</p>"
            webView.loadHTMLString(html, baseURL: nil)
            handler(nil)
            return
        }
        
        typealias RenderFunc = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
        typealias FreeFunc = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
        
        let folio_render_html = unsafeBitCast(symRender, to: RenderFunc.self)
        let folio_free_string = unsafeBitCast(symFree, to: FreeFunc.self)
        
        do {
            let fileData = try Data(contentsOf: url)
            guard let contentStr = String(data: fileData, encoding: .utf8) else {
                let html = "<h1>Encoding Error</h1><p>The file is not a valid UTF-8 text file.</p>"
                webView.loadHTMLString(html, baseURL: nil)
                handler(nil)
                return
            }
            
            guard let resultPtr = folio_render_html(contentStr) else {
                let html = "<h1>Render Error</h1><p>Folio library returned null.</p>"
                webView.loadHTMLString(html, baseURL: nil)
                handler(nil)
                return
            }
            
            defer { folio_free_string(resultPtr) }
            
            let resultHtml = String(cString: resultPtr)
            webView.loadHTMLString(resultHtml, baseURL: nil)
            handler(nil)
            
        } catch {
            let html = "<h1>File Access Error</h1><pre>\(error.localizedDescription)</pre>"
            webView.loadHTMLString(html, baseURL: nil)
            handler(nil)
        }
    }
}
