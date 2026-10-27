//! Bounds on the actual managed brush, before live preview or command dispatch.
use photocraft_engine::{EngineError, Session};
use serde_json::Value;

fn denied() -> EngineError {
    EngineError::Other("This action is unavailable in this document.".into())
}

fn stroke(s: &Session, params: &Value) -> bool {
    let Some(doc) = s.active().map(|state| &state.doc) else {
        return false;
    };
    let Some(params) = params.as_object() else {
        return false;
    };
    if params.keys().any(|key| {
        !matches!(
            key.as_str(),
            "points" | "erase" | "zoom" | "target" | "clear" | "autoErase" | "block" | "seed"
        )
    }) || params
        .get("target")
        .is_some_and(|target| target != "pixels")
        || params.get("zoom").is_some_and(|zoom| {
            zoom.as_f64()
                .is_none_or(|n| !n.is_finite() || n <= 0.0 || n > 64.0)
        })
    {
        return false;
    }
    let brush = &s.tools.brush;
    if !brush.size.is_finite()
        || brush.size <= 0.0
        || brush.size > 128.0
        || brush.tip.bitmap_size().is_some()
        || brush.texture.enabled
        || brush.dual_brush.enabled
        || brush.scattering.enabled
        || brush.build_up
        || !s
            .tools
            .foreground
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        || !s
            .tools
            .background
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    {
        return false;
    }
    let Some(points) = params.get("points").and_then(Value::as_array) else {
        return false;
    };
    !points.is_empty()
        && points.len() <= 4096
        && points.iter().all(|point| {
            let coordinates = if let Some(p) = point.as_array() {
                if !(2..=8).contains(&p.len())
                    || p.iter().any(|v| v.as_f64().is_none_or(|n| !n.is_finite()))
                {
                    return false;
                }
                p[0].as_f64().zip(p[1].as_f64())
            } else if let Some(p) = point.as_object() {
                if p.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "x" | "y" | "pressure" | "tiltX" | "tiltY" | "rotation" | "time" | "wheel"
                    )
                }) || p
                    .values()
                    .any(|v| v.as_f64().is_none_or(|n| !n.is_finite()))
                {
                    return false;
                }
                p.get("x")
                    .and_then(Value::as_f64)
                    .zip(p.get("y").and_then(Value::as_f64))
            } else {
                None
            };
            coordinates.is_some_and(|(x, y)| {
                x >= 0.0
                    && y >= 0.0
                    && x < f64::from(doc.size.width)
                    && y < f64::from(doc.size.height)
            })
        })
}

/// # Errors
/// Refuses unsupported commands, paused input and unbounded/external brushes.
pub fn command_policy(s: &Session, id: &str, params: &Value) -> photocraft_engine::Result<()> {
    let allowed = !s.managed_frozen
        && match id {
            "paint.stroke" | "paint.pencil" => stroke(s, params),
            "image.imageRotation.flipCanvasHorizontal"
            | "image.imageRotation.flipCanvasVertical"
            | "image.imageRotation.90cw"
            | "image.imageRotation.90ccw"
            | "image.imageRotation.180"
            | "edit.undo"
            | "edit.redo" => {
                // Upstream menu enablement uses Null; the actual click sends {}.
                // Both spell an absent argument list for these exact commands.
                params.is_null() || params.as_object().is_some_and(serde_json::Map::is_empty)
            }
            "tools.setColors" | "view.zoomIn" | "view.zoomOut" | "view.fitOnScreen"
            | "view.100" | "view.zoom" => true,
            _ => false,
        };
    if allowed { Ok(()) } else { Err(denied()) }
}
