pub mod arena;
pub mod styles;
pub mod resolver;
pub mod layout;
pub mod text;
pub mod paginate;
pub mod backend;
pub mod counters;
pub mod introspection;

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

/// Полный pipeline: Document → bytes выбранного формата.
///
/// 1. Resolver:  AST → StyledTree (Arena)
/// 2. Layout:    StyledTree → LayoutTree (taffy)
/// 3. Paginate:  LayoutTree → PageTree (A4 pages)
/// 4. Backend:   PageTree → export bytes (PDF/SVG)
pub fn render(doc: &Document, options: ExportOptions) -> Vec<u8> {
    let key = render_cache_key(doc, options);
    if let Some(cached) = render_cache().lock().ok().and_then(|m| m.get(&key).cloned()) {
        return cached;
    }

    let styled = resolver::build_styled_tree(doc);
    let _heading_counters = counters::collect_heading_counters(&styled);
    let layout = layout::compute_layout(&styled);
    let pages  = paginate::paginate(&layout, &styled);
    let _introspection = introspection::build_page_introspection(&layout, &pages);

    let bytes = match options.format {
        ExportFormat::Pdf => backend::pdf::render(&pages),
        ExportFormat::Svg => backend::svg::render(&pages).into_bytes(),
    };

    cache_render(key, &bytes);
    bytes
}

/// Полный pipeline: Document → PDF bytes
pub fn render_pdf(doc: &Document) -> Vec<u8> {
    render(doc, ExportOptions { format: ExportFormat::Pdf })
}

/// Полный pipeline: Document → SVG string
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

