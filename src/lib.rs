pub mod lexer;
pub mod parser;
pub mod renderer;
pub mod engine;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// # Safety
///
/// `content_ptr` must be a valid, NUL-terminated pointer to a UTF-8 string that remains valid
/// for the duration of this call (typically static or heap memory owned by the caller until
/// the function returns).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn folio_render_html(content_ptr: *const c_char) -> *mut c_char {
    if content_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let c_str = unsafe { CStr::from_ptr(content_ptr) };
    let content = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let mut lex_engine = lexer::Lexer::new(content);
    let tokens = lex_engine.tokenize();
    let mut parse_engine = parser::Parser::new(tokens);

    let mut doc = match parse_engine.parse() {
        Ok(d) => d,
        Err(e) => {
            let safe = e.replace('\0', "\u{FFFD}");
            let body = renderer::html::escape_html(&safe);
            let error_html = format!("<h1>Parse Error</h1><pre>{body}</pre>");
            return match CString::new(error_html) {
                Ok(c) => c.into_raw(),
                Err(_) => std::ptr::null_mut(),
            };
        }
    };

    doc = parser::resolver::resolve(doc);
    doc = parser::id::assign_ids(doc);
    let html = renderer::html::render(&doc);

    match CString::new(html) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// # Safety
///
/// `ptr` must be either null or a pointer previously returned by [`folio_render_html`] and not
/// yet freed; double-free or passing any other pointer is undefined behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn folio_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr); }
    }
}
