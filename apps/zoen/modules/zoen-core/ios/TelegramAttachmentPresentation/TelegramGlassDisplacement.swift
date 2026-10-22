// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

private func a(_ a1: CGFloat, _ a2: CGFloat) -> CGFloat
{
    return 1.0 - 3.0 * a2 + 3.0 * a1
}

private func b(_ a1: CGFloat, _ a2: CGFloat) -> CGFloat
{
    return 3.0 * a2 - 6.0 * a1
}

private func c(_ a1: CGFloat) -> CGFloat
{
    return 3.0 * a1
}

private func calcBezier(_ t: CGFloat, _ a1: CGFloat, _ a2: CGFloat) -> CGFloat
{
    return ((a(a1, a2)*t + b(a1, a2))*t + c(a1)) * t
}

private func calcSlope(_ t: CGFloat, _ a1: CGFloat, _ a2: CGFloat) -> CGFloat
{
    return 3.0 * a(a1, a2) * t * t + 2.0 * b(a1, a2) * t + c(a1)
}

private func getTForX(_ x: CGFloat, _ x1: CGFloat, _ x2: CGFloat) -> CGFloat {
    var t = x
    var i = 0
    while i < 4 {
        let currentSlope = calcSlope(t, x1, x2)
        if currentSlope == 0.0 {
            return t
        } else {
            let currentX = calcBezier(t, x1, x2) - x
            t -= currentX / currentSlope
        }
        
        i += 1
    }
    
    return t
}

private func meshDisplacementBezierPoint(_ x1: CGFloat, _ y1: CGFloat, _ x2: CGFloat, _ y2: CGFloat, _ x: CGFloat) -> CGFloat
{
    var value = calcBezier(getTForX(x, x1, x2), y1, y2)
    if value >= 0.997 {
        value = 1.0
    }
    return value
}

/// Bezier control points for displacement easing curve
public struct DisplacementBezier {
    var x1: CGFloat
    var y1: CGFloat
    var x2: CGFloat
    var y2: CGFloat

    public init(x1: CGFloat, y1: CGFloat, x2: CGFloat, y2: CGFloat) {
        self.x1 = x1
        self.y1 = y1
        self.x2 = x2
        self.y2 = y2
    }
}


public func roundedRectSDF(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat, cornerRadius: CGFloat) -> CGFloat {
    // Center the point (SDF formula assumes center at origin)
    let px = x - width / 2
    let py = y - height / 2

    // Half extents of the box
    let bx = width / 2
    let by = height / 2

    // Standard rounded box SDF (Inigo Quilez formula)
    let qx = abs(px) - bx + cornerRadius
    let qy = abs(py) - by + cornerRadius

    let outsideDist = hypot(max(qx, 0), max(qy, 0))
    let insideDist = min(max(qx, qy), 0)

    return outsideDist + insideDist - cornerRadius
}

/// Computes the gradient (outward normal) of the rounded rect SDF.
/// Returns normalized direction perpendicular to the nearest edge point.
public func roundedRectGradient(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat, cornerRadius: CGFloat) -> (nx: CGFloat, ny: CGFloat) {
    // Center the point
    let px = x - width / 2
    let py = y - height / 2

    // Half extents
    let bx = width / 2
    let by = height / 2

    // q values from SDF formula
    let qx = abs(px) - bx + cornerRadius
    let qy = abs(py) - by + cornerRadius

    var nx: CGFloat = 0
    var ny: CGFloat = 0

    if qx > 0 && qy > 0 {
        // Corner region - normal points radially from corner arc center
        let d = hypot(qx, qy)
        if d > 0 {
            nx = qx / d
            ny = qy / d
        }
    } else if qx > qy {
        // Nearest point is on vertical edge (left or right)
        nx = 1
        ny = 0
    } else {
        // Nearest point is on horizontal edge (top or bottom)
        nx = 0
        ny = 1
    }

    // Restore sign based on which side of center we're on
    if px < 0 { nx = -nx }
    if py < 0 { ny = -ny }

    return (nx, ny)
}

/// Computes displacement at a point analytically using the rounded rect SDF.
/// - Parameters:
///   - x, y: Point coordinates in the shape's coordinate space
///   - width, height: Dimensions of the rounded rectangle
///   - cornerRadius: Already-clamped corner radius
///   - edgeDistance: Distance (in points) over which displacement fades from edge inward
///   - bezier: Bezier control points for easing the displacement magnitude
/// - Returns: (dx, dy) displacement in range -1..1 with bezier easing, plus the raw SDF value
public func computeDisplacement(
    x: CGFloat,
    y: CGFloat,
    width: CGFloat,
    height: CGFloat,
    cornerRadius: CGFloat,
    edgeDistance: CGFloat,
    bezier: DisplacementBezier
) -> (dx: CGFloat, dy: CGFloat, sdf: CGFloat) {
    let sdf = roundedRectSDF(x: x, y: y, width: width, height: height, cornerRadius: cornerRadius)
    let (nx, ny) = roundedRectGradient(x: x, y: y, width: width, height: height, cornerRadius: cornerRadius)

    // Inward normal (content moves away from edge, toward center)
    let inwardX = -nx
    let inwardY = -ny

    // Distance from edge (positive inside the shape)
    let distFromEdge = -sdf

    // Weight: 1 at edge, 0 at edgeDistance inward (linear falloff)
    let weight = max(0, min(1, 1.0 - distFromEdge / edgeDistance))

    // Displacement direction modulated by distance
    var dx = inwardX * weight
    var dy = inwardY * weight

    // Apply bezier easing to vector magnitude, preserving direction
    let mag = hypot(dx, dy)
    if mag > 0 {
        let newMag = meshDisplacementBezierPoint(bezier.x1, bezier.y1, bezier.x2, bezier.y2, mag)
        let scale = newMag / mag
        dx *= scale
        dy *= scale
    }

    return (dx, dy, sdf)
}

/// Generates a glass mesh with corner-aware topology.
/// - 4 radial corner wedges sampled in polar space
/// - 4 edge strips aligned with the rectangle sides
/// - 1 center patch
/// Corner/edge seams share the same coordinates (but do not reuse vertices) so
/// the neighbouring faces fit perfectly without T-junctions.
