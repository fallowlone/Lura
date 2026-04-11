use super::painter::{PaintDocument, PainterBackend};

/// Scaffold for a future GPU preview backend.
/// Enabled by the `wgpu-preview` feature flag.
pub struct WgpuBackend;

impl PainterBackend for WgpuBackend {
    fn render_document(&self, _doc: &PaintDocument) -> Vec<u8> {
        todo!("WGPU preview backend is not implemented yet")
    }
}
