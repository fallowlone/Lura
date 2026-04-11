pub mod lexer;
pub mod parser;
pub mod renderer;
pub mod engine;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Outcome of [`lura_render_pdf`]: either PDF bytes or a UTF-8 error message (parse/layout).
///
/// Callers must pass the pointer to [`lura_free_pdf_result`] when done. Do not free fields
/// individually.
#[repr(C)]
pub struct LuraPdfResult {
    pub pdf_ptr: *mut u8,
    pub pdf_len: usize,
    /// Capacity of the allocation behind `pdf_ptr` (for Rust `Vec` reconstruction on free).
    pub pdf_cap: usize,
    /// NUL-terminated UTF-8 error message when `pdf_len == 0`; null on success.
    pub error_ptr: *mut c_char,
}

/// # Safety
///
/// `content_ptr` must be a valid, NUL-terminated pointer to a UTF-8 string that remains valid
/// for the duration of this call.
///
/// Returns a heap-allocated [`LuraPdfResult`] or null only on catastrophic allocation failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lura_render_pdf(content_ptr: *const c_char) -> *mut LuraPdfResult {
    if content_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let c_str = unsafe { CStr::from_ptr(content_ptr) };
    let content = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => {
            let msg = CString::new("Document is not valid UTF-8").unwrap_or_else(|_| {
                CString::new("Invalid UTF-8").expect("static C string")
            });
            return Box::into_raw(Box::new(LuraPdfResult {
                pdf_ptr: std::ptr::null_mut(),
                pdf_len: 0,
                pdf_cap: 0,
                error_ptr: msg.into_raw(),
            }));
        }
    };

    Box::into_raw(Box::new(parse_and_render_pdf(content)))
}

fn parse_and_render_pdf(content: &str) -> LuraPdfResult {
    let mut lex_engine = lexer::Lexer::new(content);
    let tokens = lex_engine.tokenize();
    let mut parse_engine = parser::Parser::new(tokens);

    let mut doc = match parse_engine.parse() {
        Ok(d) => d,
        Err(e) => {
            let safe = e.replace('\0', "\u{FFFD}");
            let c_err = CString::new(safe).unwrap_or_else(|_| {
                CString::new("Parse error").expect("static C string")
            });
            return LuraPdfResult {
                pdf_ptr: std::ptr::null_mut(),
                pdf_len: 0,
                pdf_cap: 0,
                error_ptr: c_err.into_raw(),
            };
        }
    };

    doc = parser::resolver::resolve(doc);
    doc = parser::id::assign_ids(doc);
    let pdf = engine::render_pdf(&doc);

    let (pdf_ptr, pdf_len, pdf_cap) = if pdf.is_empty() {
        let msg = CString::new("Layout produced an empty PDF").unwrap_or_else(|_| {
            CString::new("Empty PDF").expect("static C string")
        });
        return LuraPdfResult {
            pdf_ptr: std::ptr::null_mut(),
            pdf_len: 0,
            pdf_cap: 0,
            error_ptr: msg.into_raw(),
        };
    } else {
        let mut pdf = pdf;
        let pdf_len = pdf.len();
        let pdf_cap = pdf.capacity();
        let pdf_ptr = pdf.as_mut_ptr();
        std::mem::forget(pdf);
        (pdf_ptr, pdf_len, pdf_cap)
    };

    LuraPdfResult {
        pdf_ptr,
        pdf_len,
        pdf_cap,
        error_ptr: std::ptr::null_mut(),
    }
}

/// # Safety
///
/// `ptr` must be null or the only pointer returned from [`lura_render_pdf`] for that result,
/// not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lura_free_pdf_result(ptr: *mut LuraPdfResult) {
    if ptr.is_null() {
        return;
    }
    let result = unsafe { Box::from_raw(ptr) };
    if !result.pdf_ptr.is_null() && result.pdf_cap > 0 {
        unsafe {
            drop(Vec::from_raw_parts(
                result.pdf_ptr,
                result.pdf_len,
                result.pdf_cap,
            ));
        }
    }
    if !result.error_ptr.is_null() {
        unsafe {
            drop(CString::from_raw(result.error_ptr));
        }
    }
}

#[cfg(test)]
mod ffi_tests {
    use super::*;

    #[test]
    fn lura_render_pdf_valid_document_starts_with_pdf_magic() {
        let src = CString::new("PAGE(P(Hello))").unwrap();
        let ptr = unsafe { lura_render_pdf(src.as_ptr()) };
        assert!(!ptr.is_null(), "expected result");
        unsafe {
            let r = &*ptr;
            assert!(r.error_ptr.is_null(), "unexpected error");
            assert!(r.pdf_len >= 4, "pdf too short");
            let header = std::slice::from_raw_parts(r.pdf_ptr, 4);
            assert_eq!(header, b"%PDF");
            lura_free_pdf_result(ptr);
        }
    }

    #[test]
    fn lura_render_pdf_parse_error_sets_error_ptr() {
        let src = CString::new("PAGE(").unwrap();
        let ptr = unsafe { lura_render_pdf(src.as_ptr()) };
        assert!(!ptr.is_null());
        unsafe {
            let r = &*ptr;
            assert_eq!(r.pdf_len, 0);
            assert!(!r.error_ptr.is_null());
            let msg = CStr::from_ptr(r.error_ptr).to_str().unwrap();
            assert!(!msg.is_empty());
            lura_free_pdf_result(ptr);
        }
    }
}
