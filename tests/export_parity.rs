use lura::engine::backend::painter::capability_matrix;

#[test]
fn pdf_svg_capabilities_are_parity_baseline() {
    let matrix = capability_matrix();
    assert!(matrix.len() >= 2);
    let pdf = matrix.iter().find(|(name, _)| *name == "pdf").expect("pdf row").1;
    let svg = matrix.iter().find(|(name, _)| *name == "svg").expect("svg row").1;
    assert_eq!(pdf.text, svg.text);
    assert_eq!(pdf.mixed_inline, svg.mixed_inline);
    assert_eq!(pdf.rect, svg.rect);
    assert_eq!(pdf.line, svg.line);

    #[cfg(feature = "wgpu-preview")]
    if let Some(wgpu_row) = matrix.iter().find(|(name, _)| *name == "wgpu") {
        let w = wgpu_row.1;
        assert_eq!(pdf.text, w.text);
        assert_eq!(pdf.mixed_inline, w.mixed_inline);
        assert_eq!(pdf.rect, w.rect);
        assert_eq!(pdf.line, w.line);
    }
}
