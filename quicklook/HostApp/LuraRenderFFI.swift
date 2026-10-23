import Darwin
import Foundation

/// Either a decoded text index or an error message from extract_text. Avoids
/// introducing a bespoke `Error` type for what is a simple success/failure pair.
enum LuraTextIndexOutcome {
    case success([LuraTextUnit])
    case failure(String)
}

/// One painted line of the rendered document, in bottom-origin PDF points
/// (matches the `TextUnit` shape emitted by `engine::extract_text_index`).
struct LuraTextUnit: Hashable {
    let page: Int
    let x: CGFloat
    let y: CGFloat
    let w: CGFloat
    let h: CGFloat
    let text: String
    let blockId: String
}

/// Loads `liblura.dylib` from the host app bundle once and calls `lura_render_pdf` on the main thread.
enum LuraRenderFFI {
    private static var handle: UnsafeMutableRawPointer?
    private static var symRender: UnsafeMutableRawPointer?
    private static var symFree: UnsafeMutableRawPointer?
    private static var symExtract: UnsafeMutableRawPointer?
    private static var symFreeExtract: UnsafeMutableRawPointer?

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
        symExtract = dlsym(h, "lura_extract_text")
        symFreeExtract = dlsym(h, "lura_free_text_index_result")
        if symRender == nil || symFree == nil {
            return "Missing lura_render_pdf or lura_free_pdf_result in liblura.dylib."
        }
        if symExtract == nil || symFreeExtract == nil {
            return "Missing lura_extract_text or lura_free_text_index_result in liblura.dylib."
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

    static func extractText(source: String) -> LuraTextIndexOutcome {
        if let err = loadLibrary() {
            return .failure(err)
        }
        let out = LuraPdfFFI.invokeExtractText(
            source: source,
            symExtract: symExtract!,
            symFree: symFreeExtract!
        )
        if let err = out.errorMessage {
            return .failure(err)
        }
        guard let data = out.jsonData else {
            return .failure("Empty text index payload.")
        }
        return decodeTextUnits(data)
    }

    private static func decodeTextUnits(_ data: Data) -> LuraTextIndexOutcome {
        struct Payload: Decodable {
            struct Unit: Decodable {
                let page: Int
                let x: Double
                let y: Double
                let w: Double
                let h: Double
                let text: String
                let block_id: String
            }
            let units: [Unit]
        }
        do {
            let payload = try JSONDecoder().decode(Payload.self, from: data)
            let units = payload.units.map {
                LuraTextUnit(
                    page: $0.page,
                    x: CGFloat($0.x),
                    y: CGFloat($0.y),
                    w: CGFloat($0.w),
                    h: CGFloat($0.h),
                    text: $0.text,
                    blockId: $0.block_id
                )
            }
            return .success(units)
        } catch {
            return .failure("Failed to decode text index JSON: \(error.localizedDescription)")
        }
    }
}
