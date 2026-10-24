//! Unicode extraction adapted from pinned WordCraft PDF tests (MIT OR Apache-2.0).
/// Text of every page through hayro's interpreter (glyph → Unicode via ToUnicode / ActualText).
pub(super) fn extract_text(bytes: &[u8]) -> crate::Result<Vec<String>> {
    use hayro_interpret::font::Glyph;
    use hayro_interpret::{
        BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache,
        InterpreterSettings, Paint, PathDrawMode, SoftMask, interpret_page,
    };
    use hayro_syntax::Pdf;
    struct Ex(String);
    impl Device<'_> for Ex {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(
            &mut self,
            _: &kurbo::BezPath,
            _: kurbo::Affine,
            _: &Paint<'_>,
            _: &PathDrawMode,
        ) {
        }
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(
            &mut self,
            g: &Glyph<'_>,
            _: kurbo::Affine,
            _: kurbo::Affine,
            _: &Paint<'_>,
            mode: &GlyphDrawMode,
        ) {
            // Synthetic bold fills and then strokes each glyph: one character, two draws.
            if matches!(mode, GlyphDrawMode::Stroke(_)) {
                return;
            }
            match g.as_unicode() {
                Some(hayro_cmap::BfString::Char(c)) => self.0.push(c),
                Some(hayro_cmap::BfString::String(s)) => self.0.push_str(&s),
                None => self.0.push('\u{FFFD}'),
            }
        }
        fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }
    let pdf = Pdf::new(bytes.to_vec()).map_err(|_| crate::failure("invalid_generated_pdf"))?;
    let cache = InterpreterCache::new();
    Ok(pdf
        .pages()
        .iter()
        .map(|page| {
            let mut ctx = Context::new(
                kurbo::Affine::IDENTITY,
                kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
                &cache,
                pdf.xref(),
                InterpreterSettings::default(),
            );
            let mut ex = Ex(String::new());
            interpret_page(page, &mut ctx, &mut ex);
            ex.0
        })
        .collect())
}
