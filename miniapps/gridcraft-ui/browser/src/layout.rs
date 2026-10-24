use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub insets: Insets,
    pub close_button: Option<Rect>,
    pub theme: Option<Theme>,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}
impl Layout {
    #[must_use]
    pub fn valid(&self) -> bool {
        let i = self.insets;
        if ![self.width, self.height]
            .iter()
            .all(|n| n.is_finite() && *n >= 1.0 && *n <= 16_384.0)
            || ![i.top, i.right, i.bottom, i.left]
                .iter()
                .all(|n| n.is_finite() && *n >= 0.0)
            || i.left + i.right >= self.width
            || i.top + i.bottom >= self.height
        {
            return false;
        }
        self.close_button.is_none_or(|r| {
            [r.x, r.y, r.width, r.height]
                .iter()
                .all(|n| n.is_finite() && *n >= 0.0)
                && r.width > 0.0
                && r.height > 0.0
                && r.x + r.width <= self.width
                && r.y + r.height + 4.0 < self.height - i.bottom
        })
    }
    #[must_use]
    pub fn safe_insets(&self) -> egui::SafeAreaInsets {
        // The host owns the floating close button; keep the editor's top controls
        // out of its measured rectangle, including after rotation/keyboard resize.
        let top = self.close_button.map_or(self.insets.top, |r| {
            self.insets.top.max(r.y + r.height + 4.0)
        });
        egui::SafeAreaInsets(egui::epaint::MarginF32 {
            top,
            right: self.insets.right,
            bottom: self.insets.bottom,
            left: self.insets.left,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_close_geometry_and_keyboard_keep_positive_safe_content() {
        for (width, height) in [(390.0, 844.0), (844.0, 390.0), (390.0, 460.0)] {
            let l = Layout {
                width,
                height,
                insets: Insets {
                    top: 24.0,
                    right: 0.0,
                    bottom: 20.0,
                    left: 0.0,
                },
                close_button: Some(Rect {
                    x: width - 56.0,
                    y: 24.0,
                    width: 44.0,
                    height: 44.0,
                }),
                theme: None,
            };
            assert!(l.valid());
            let i = l.safe_insets();
            assert_eq!(i.0.top, 72.0);
            assert!(height - i.0.top - i.0.bottom > 0.0);
        }
    }
    #[test]
    fn invalid_or_unknown_geometry_never_enters_the_renderer() {
        assert!(serde_json::from_str::<Layout>(r#"{"width":390,"height":844,"insets":{"top":0,"right":0,"bottom":0,"left":0},"theme":"other"}"#).is_err());
        let mut l = Layout {
            width: 390.0,
            height: 844.0,
            insets: Insets {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            close_button: None,
            theme: None,
        };
        assert!(l.valid());
        l.width = f32::NAN;
        assert!(!l.valid());
        l.width = 390.0;
        l.insets.left = 390.0;
        assert!(!l.valid());
        l.insets.left = 0.0;
        l.close_button = Some(Rect {
            x: 380.0,
            y: 0.0,
            width: 44.0,
            height: 44.0,
        });
        assert!(!l.valid());
    }
}
