use super::painter::{PaintDocument, PainterBackend};

/// Каркас будущего GPU preview backend.
/// Включается feature-флагом `wgpu-preview`.
pub struct WgpuBackend;

impl PainterBackend for WgpuBackend {
    fn render_document(&self, _doc: &PaintDocument) -> Vec<u8> {
        todo!("WGPU preview backend is not implemented yet")
    }
}
