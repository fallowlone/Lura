use super::painter::{from_page_tree, PaintDocument, PainterBackend, PainterCommand};
use crate::engine::paginate::PageTree;
use crate::engine::styles::Color;

pub fn render(page_tree: &PageTree) -> String {
    let doc = from_page_tree(page_tree);
    let bytes = SvgBackend.render_document(&doc);
    String::from_utf8(bytes).unwrap_or_else(|_| String::new())
}

pub struct SvgBackend;

impl PainterBackend for SvgBackend {
    fn render_document(&self, doc: &PaintDocument) -> Vec<u8> {
        let mut out = String::new();
        out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        out.push('\n');

        if doc.pages.is_empty() {
            out.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#);
            return out.into_bytes();
        }

        let width = doc.pages.iter().fold(0.0f32, |acc, p| acc.max(p.width));
        let total_height: f32 = doc.pages.iter().map(|p| p.height).sum();
        let page_gap = 12.0f32;
        let total_gap = page_gap * (doc.pages.len().saturating_sub(1) as f32);
        let canvas_height = total_height + total_gap;

        let svg_open = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{:.2}" height="{:.2}" viewBox="0 0 {:.2} {:.2}">"#,
            width, canvas_height, width, canvas_height
        );

        let mut body = String::new();
        let mut defs = String::new();
        let mut clip_seq = 0u32;

        let mut y_offset = 0.0f32;
        for page in &doc.pages {
            body.push_str(&format!(r#"<g transform="translate(0,{:.2})">"#, y_offset));
            body.push('\n');
            body.push_str(&format!(
                r##"<rect x="0" y="0" width="{:.2}" height="{:.2}" fill="white" stroke="#e5e7eb" stroke-width="1"/>"##,
                page.width, page.height
            ));
            body.push('\n');

            for cmd in &page.commands {
                match cmd {
                    PainterCommand::PushOpacity { alpha } => {
                        body.push_str(&format!(
                            r#"<g opacity="{:.4}">"#,
                            alpha.clamp(0.0, 1.0)
                        ));
                        body.push('\n');
                    }
                    PainterCommand::PopOpacity => {
                        body.push_str("</g>\n");
                    }
                    PainterCommand::PushClipRect { x, y, w, h } => {
                        let id = clip_seq;
                        clip_seq += 1;
                        defs.push_str(&format!(
                            r#"<clipPath id="lc{}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/></clipPath>"#,
                            id, x, y, w, h
                        ));
                        defs.push('\n');
                        body.push_str(&format!(r#"<g clip-path="url(#lc{})">"#, id));
                        body.push('\n');
                    }
                    PainterCommand::PopClip => {
                        body.push_str("</g>\n");
                    }
                    PainterCommand::Rect { x, y, w, h, fill, stroke, stroke_width } => {
                        let fill_attr = fill
                            .map(color_to_svg)
                            .unwrap_or_else(|| "none".to_string());
                        let stroke_attr = stroke
                            .map(color_to_svg)
                            .unwrap_or_else(|| "none".to_string());
                        body.push_str(&format!(
                            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}" stroke="{}" stroke-width="{:.2}"/>"#,
                            x, y, w, h, fill_attr, stroke_attr, stroke_width
                        ));
                        body.push('\n');
                    }
                    PainterCommand::Line { x1, y1, x2, y2, color, width } => {
                        body.push_str(&format!(
                            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{:.2}" />"#,
                            x1, y1, x2, y2, color_to_svg(*color), width
                        ));
                        body.push('\n');
                    }
                    PainterCommand::Text { content, x, y, font_size, font_family, bold, italic, color } => {
                        let mut attrs = Vec::new();
                        attrs.push(format!(r#"font-family="{}""#, escape_xml(font_family)));
                        attrs.push(format!(r#"font-size="{:.2}""#, font_size));
                        if *bold {
                            attrs.push(r#"font-weight="700""#.to_string());
                        }
                        if *italic {
                            attrs.push(r#"font-style="italic""#.to_string());
                        }
                        attrs.push(format!(r#"fill="{}""#, color_to_svg(*color)));
                        attrs.push(r#"xml:space="preserve""#.to_string());
                        body.push_str(&format!(
                            r#"<text x="{:.2}" y="{:.2}" {}>{}</text>"#,
                            x,
                            y,
                            attrs.join(" "),
                            escape_xml(content)
                        ));
                        body.push('\n');
                    }
                }
            }

            body.push_str("</g>\n");
            y_offset += page.height + page_gap;
        }

        out.push_str(&svg_open);
        out.push('\n');
        if !defs.is_empty() {
            out.push_str("<defs>\n");
            out.push_str(&defs);
            out.push_str("</defs>\n");
        }
        out.push_str(&body);
        out.push_str("</svg>\n");
        out.into_bytes()
    }
}

fn color_to_svg(c: Color) -> String {
    format!(
        "rgb({},{},{})",
        (c.r * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.g * 255.0).round().clamp(0.0, 255.0) as u8,
        (c.b * 255.0).round().clamp(0.0, 255.0) as u8
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
