pub mod arena;
pub mod counters;
pub mod grid_tracks;
pub mod introspection;
pub mod styles;
pub mod resolver;
pub mod layout;
pub mod text;
pub mod paginate;
pub mod backend;

#[cfg(test)]
mod pipeline_tests;

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use crate::parser::ast::{Content, Document, NodeId, Value};

static RENDER_CACHE: OnceLock<Mutex<HashMap<u64, Vec<u8>>>> = OnceLock::new();
const RENDER_CACHE_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Pdf,
    Svg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExportOptions {
    pub format: ExportFormat,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { format: ExportFormat::Pdf }
    }
}

/// Full pipeline: `Document` → bytes for the chosen format.
///
/// 1. Resolver:  AST → StyledTree (Arena)
/// 2. Counters:  outline numbers + `{{sec}}` in headings (`docs/SPEC.md`)
/// 3. Layout:    StyledTree → LayoutTree (taffy)
/// 4. Paginate:  LayoutTree → PageTree (A4 pages), optional `{{page:id}}` passes
/// 5. Backend:   PageTree → export bytes (PDF/SVG)
pub fn render(doc: &Document, options: ExportOptions) -> Vec<u8> {
    let key = render_cache_key(doc, options);
    if let Some(cached) = render_cache().lock().ok().and_then(|m| m.get(&key).cloned()) {
        return cached;
    }

    let mut styled = resolver::build_styled_tree(doc);
    let heading_nums = counters::compute_heading_numbers(&styled);
    counters::apply_sec_placeholders(&mut styled, &heading_nums);

    const MAX_PAGE_PASSES: usize = 5;
    let mut page_tree = paginate::PageTree {
        pages: Vec::new(),
        block_start_page: std::collections::HashMap::new(),
    };
    // After the first `apply_page_placeholders`, `{{page:…}}` is gone; we must not use that
    // to exit the loop or we never reflow with substituted digits and never compare fingerprints.
    let needs_page_passes = introspection::arena_has_page_placeholders(&styled);
    let mut prev_fp: Option<u64> = None;
    for _ in 0..MAX_PAGE_PASSES {
        let layout = layout::compute_layout(&styled);
        page_tree = paginate::paginate(&layout, &styled);
        if !needs_page_passes {
            break;
        }
        let fp = introspection::fingerprint_page_map(&page_tree.block_start_page);
        // Stable map under current (possibly already substituted) text: done.
        if prev_fp == Some(fp) {
            break;
        }
        introspection::apply_page_placeholders(&mut styled, &page_tree.block_start_page);
        prev_fp = Some(fp);
    }

    let bytes = match options.format {
        ExportFormat::Pdf => backend::pdf::render(&page_tree),
        ExportFormat::Svg => backend::svg::render(&page_tree).into_bytes(),
    };

    cache_render(key, &bytes);
    bytes
}

/// Full pipeline: `Document` → PDF bytes
pub fn render_pdf(doc: &Document) -> Vec<u8> {
    render(doc, ExportOptions { format: ExportFormat::Pdf })
}

/// Full pipeline: `Document` → SVG string
pub fn render_svg(doc: &Document) -> String {
    String::from_utf8(render(doc, ExportOptions { format: ExportFormat::Svg }))
        .unwrap_or_else(|_| String::new())
}

fn render_cache() -> &'static Mutex<HashMap<u64, Vec<u8>>> {
    RENDER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_render(key: u64, value: &[u8]) {
    if let Ok(mut map) = render_cache().lock() {
        if map.len() >= RENDER_CACHE_LIMIT {
            map.clear();
        }
        map.insert(key, value.to_vec());
    }
}

fn render_cache_key(doc: &Document, options: ExportOptions) -> u64 {
    let mut hasher = DefaultHasher::new();
    document_fingerprint(doc).hash(&mut hasher);
    options.hash(&mut hasher);
    hasher.finish()
}

fn document_fingerprint(doc: &Document) -> u64 {
    let mut hasher = DefaultHasher::new();

    let mut vars: Vec<_> = doc.vars.iter().collect();
    vars.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
    for (k, v) in vars {
        k.hash(&mut hasher);
        hash_value(v, &mut hasher);
    }

    for &root in doc.root_ids() {
        hash_block(doc, root, &mut hasher);
    }
    hasher.finish()
}

fn hash_block(doc: &Document, id: NodeId, hasher: &mut DefaultHasher) {
    let block = doc.block(id);
    block.kind.hash(hasher);
    block.id.hash(hasher);

    let mut attrs: Vec<_> = block.attrs.iter().collect();
    attrs.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
    for (k, v) in attrs {
        k.hash(hasher);
        hash_value(v, hasher);
    }

    match &block.content {
        Content::Text(t) => {
            1u8.hash(hasher);
            t.hash(hasher);
        }
        Content::Inline(nodes) => {
            4u8.hash(hasher);
            Document::inline_text(nodes).hash(hasher);
        }
        Content::Children(children) => {
            2u8.hash(hasher);
            for &child in children {
                hash_block(doc, child, hasher);
            }
        }
        Content::Empty => {
            3u8.hash(hasher);
        }
    }
}

fn hash_value(v: &Value, hasher: &mut DefaultHasher) {
    match v {
        Value::Str(s) => {
            1u8.hash(hasher);
            s.hash(hasher);
        }
        Value::Number(n) => {
            2u8.hash(hasher);
            n.to_bits().hash(hasher);
        }
        Value::Unit(n, u) => {
            3u8.hash(hasher);
            n.to_bits().hash(hasher);
            u.hash(hasher);
        }
        Value::Var(s) => {
            4u8.hash(hasher);
            s.hash(hasher);
        }
        Value::Color(s) => {
            5u8.hash(hasher);
            s.hash(hasher);
        }
    }
}

