//! Closed agent vocabulary shared by native review and the actual editor adapter.
use crate::{validate_text, Result};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "command", content = "params", deny_unknown_fields)]
pub enum Action {
    #[serde(rename = "text.replace")]
    TextReplace(TextReplace),
    #[serde(rename = "text.edit")]
    TextEdit(TextEdit),
    #[serde(rename = "range.format")]
    RangeFormat(RangeFormat),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextReplace {
    pub text: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextEdit {
    pub range: Range,
    pub text: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RangeFormat {
    pub range: Range,
    pub format: Format,
}
/// Zero-based body paragraph and UTF-8 byte offset, validated before selection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub paragraph: u32,
    pub offset: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Format {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub bold: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub italic: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub underline: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub strikethrough: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub font: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub size: Option<f32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub color: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub alignment: Option<Alignment>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub paragraph_style: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub space_before: Option<f32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub space_after: Option<f32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub line_spacing: Option<f32>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    Left,
    Center,
    Right,
    Justify,
}
impl Alignment {
    #[must_use]
    pub fn command_value(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::Justify => "justify",
        }
    }
}
fn bounded_name(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}
impl Format {
    /// # Errors
    /// Refuses empty/ambiguous patches and values the engine would clamp silently.
    pub fn validate(&self) -> Result<()> {
        if self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strikethrough.is_none()
            && self.font.is_none()
            && self.size.is_none()
            && self.color.is_none()
            && self.alignment.is_none()
            && self.paragraph_style.is_none()
            && self.space_before.is_none()
            && self.space_after.is_none()
            && self.line_spacing.is_none()
        {
            return Err("empty_format");
        }
        if self.font.as_deref().is_some_and(|s| !bounded_name(s))
            || self
                .paragraph_style
                .as_deref()
                .is_some_and(|s| !bounded_name(s))
            || self.size.is_some_and(|n| {
                !n.is_finite() || (!(1.0..=1638.0).contains(&n) || (n * 2.0).fract().abs() > 0.0)
            })
            || self
                .space_before
                .into_iter()
                .chain(self.space_after)
                .any(|n| !n.is_finite() || !(0.0..=1584.0).contains(&n))
            || self
                .line_spacing
                .is_some_and(|n| !n.is_finite() || !(0.5..=10.0).contains(&n))
            || self.color.as_deref().is_some_and(|s| {
                s != "auto" && (s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            })
        {
            return Err("invalid_format");
        }
        Ok(())
    }
}
impl Action {
    /// # Errors
    /// Refuses invalid coordinates, text or style values before execution/review.
    pub fn validate(&self) -> Result<()> {
        let range = match self {
            Self::TextReplace(v) => {
                validate_text(&v.text)?;
                return Ok(());
            }
            Self::TextEdit(v) => {
                validate_text(&v.text)?;
                &v.range
            }
            Self::RangeFormat(v) => {
                v.format.validate()?;
                if v.range.start == v.range.end {
                    return Err("empty_format_range");
                }
                &v.range
            }
        };
        if range.start > range.end {
            return Err("reversed_range");
        }
        Ok(())
    }
}
