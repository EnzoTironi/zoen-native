use super::*;
use model::{
    props::{Align, CharProps, Rgb, TextColor},
    Block,
};
use std::sync::Arc;
#[test]
fn genuine_rich_runs_paragraphs_styles_and_plain_documents_keep_their_model() -> Result<()> {
    for rich in [false, true] {
        let mut document = Document::from_text("Heading\nStyled paragraph");
        if rich {
            for block in &mut document.body {
                let Block::Para(paragraph) = Arc::make_mut(block) else {
                    return Err("native paragraph missing");
                };
                paragraph.props.align = Some(Align::Center);
                for run in &mut paragraph.runs {
                    run.props = CharProps {
                        bold: Some(true),
                        italic: Some(true),
                        size: Some(18.0),
                        color: Some(TextColor::Rgb(Rgb(32, 64, 128))),
                        ..Default::default()
                    };
                }
            }
        }
        let bytes = encode(&document)?;
        assert_eq!(
            parse(&bytes)?,
            document,
            "rich formatting must never become plain text"
        );
        assert_eq!(
            encode(&parse(&bytes)?)?,
            bytes,
            "same model retains exact writer bytes"
        );
    }
    Ok(())
}
