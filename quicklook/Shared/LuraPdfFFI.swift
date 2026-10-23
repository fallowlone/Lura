import Darwin
import Foundation

/// C ABI mirror of Rust `LuraPdfResult` in `src/lib.rs` (field order must match `#[repr(C)]`).
struct LuraPdfResultC {
    var pdfPtr: UnsafeMutablePointer<UInt8>?
    var pdfLen: Int
    var pdfCap: Int
    var errorPtr: UnsafeMutablePointer<CChar>?
}

/// C ABI mirror of Rust `LuraTextIndexResult` in `src/lib.rs` (field order must
/// match `#[repr(C)]`).
struct LuraTextIndexResultC {
    var jsonPtr: UnsafeMutablePointer<UInt8>?
    var jsonLen: Int
    var jsonCap: Int
    var errorPtr: UnsafeMutablePointer<CChar>?
}

enum LuraPdfFFI {
    struct Output {
        var pdfData: Data?
        var errorMessage: String?
    }

    struct TextIndexOutput {
        var jsonData: Data?
        var errorMessage: String?
    }

    /// Calls `lura_render_pdf` / `lura_free_pdf_result` from an already-resolved dylib.
    static func invokeRender(
        source: String,
        symRender: UnsafeMutableRawPointer,
        symFree: UnsafeMutableRawPointer
    ) -> Output {
        // `@convention(c)` cannot use Optional in the parameter/return Swift types; use raw pointers.
        typealias RenderFn = @convention(c) (UnsafePointer<CChar>) -> UnsafeMutableRawPointer?
        typealias FreeFn = @convention(c) (UnsafeMutableRawPointer?) -> Void
        let render = unsafeBitCast(symRender, to: RenderFn.self)
        let freeResult = unsafeBitCast(symFree, to: FreeFn.self)

        return source.withCString { cstr in
            guard let raw = render(cstr) else {
                return Output(pdfData: nil, errorMessage: "Library returned null (out of memory).")
            }
            defer { freeResult(raw) }
            let resPtr = raw.assumingMemoryBound(to: LuraPdfResultC.self)
            let r = resPtr.pointee
            if let ep = r.errorPtr {
                return Output(pdfData: nil, errorMessage: String(cString: ep))
            }
            guard let p = r.pdfPtr, r.pdfLen > 0 else {
                return Output(pdfData: nil, errorMessage: "Empty PDF output.")
            }
            return Output(pdfData: Data(bytes: p, count: r.pdfLen), errorMessage: nil)
        }
    }

    /// Calls `lura_extract_text` / `lura_free_text_index_result` from an
    /// already-resolved dylib. Returns raw UTF-8 JSON bytes; the caller is
    /// responsible for decoding.
    static func invokeExtractText(
        source: String,
        symExtract: UnsafeMutableRawPointer,
        symFree: UnsafeMutableRawPointer
    ) -> TextIndexOutput {
        typealias ExtractFn = @convention(c) (UnsafePointer<CChar>) -> UnsafeMutableRawPointer?
        typealias FreeFn = @convention(c) (UnsafeMutableRawPointer?) -> Void
        let extractText = unsafeBitCast(symExtract, to: ExtractFn.self)
        let freeResult = unsafeBitCast(symFree, to: FreeFn.self)

        return source.withCString { cstr in
            guard let raw = extractText(cstr) else {
                return TextIndexOutput(jsonData: nil, errorMessage: "Library returned null (out of memory).")
            }
            defer { freeResult(raw) }
            let resPtr = raw.assumingMemoryBound(to: LuraTextIndexResultC.self)
            let r = resPtr.pointee
            if let ep = r.errorPtr {
                return TextIndexOutput(jsonData: nil, errorMessage: String(cString: ep))
            }
            guard let p = r.jsonPtr, r.jsonLen > 0 else {
                return TextIndexOutput(jsonData: nil, errorMessage: "Empty text index.")
            }
            return TextIndexOutput(jsonData: Data(bytes: p, count: r.jsonLen), errorMessage: nil)
        }
    }
}
