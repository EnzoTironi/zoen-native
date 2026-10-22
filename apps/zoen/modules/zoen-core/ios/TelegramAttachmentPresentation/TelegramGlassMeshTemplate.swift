// Adapted from TelegramMessenger/Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838.
// GPL-2.0-or-later; GPL-3.0 selected. See NOTICE.md and UPSTREAM_SOURCES.json.
import Foundation
import UIKit

struct GlassMeshCacheKey: Hashable {
    var cornerRadius: CGFloat
    var edgeDistance: CGFloat
    var cornerResolution: Int
    var outerEdgeDistance: CGFloat
    var bezierX1: CGFloat
    var bezierY1: CGFloat
    var bezierX2: CGFloat
    var bezierY2: CGFloat

    init(cornerRadius: CGFloat, edgeDistance: CGFloat, cornerResolution: Int, outerEdgeDistance: CGFloat, bezier: DisplacementBezier) {
        self.cornerRadius = cornerRadius
        self.edgeDistance = edgeDistance
        self.cornerResolution = cornerResolution
        self.outerEdgeDistance = outerEdgeDistance
        self.bezierX1 = bezier.x1
        self.bezierY1 = bezier.y1
        self.bezierX2 = bezier.x2
        self.bezierY2 = bezier.y2
    }
}

struct GlassMeshTemplate {
    struct VertexTemplate {
        /// worldX = baseX + sizeScaleX * width
        var baseX: CGFloat
        var sizeScaleX: CGFloat
        /// worldY = baseY + sizeScaleY * height
        var baseY: CGFloat
        var sizeScaleY: CGFloat
        /// Unitless displacement (direction * weight * bezier * edgeBoost), range roughly -1...1
        var dispX: CGFloat
        var dispY: CGFloat
        var depth: CGFloat
    }

    var vertices: ContiguousArray<VertexTemplate>
    var faces: ContiguousArray<MeshTransform.Face>
}

var glassMeshTemplateCache: [GlassMeshCacheKey: GlassMeshTemplate] = [:]

func instantiateGlassMesh(
    from template: GlassMeshTemplate,
    size: CGSize,
    displacementMagnitudeU: CGFloat,
    displacementMagnitudeV: CGFloat
) -> MeshTransform {
    let W = size.width
    let H = size.height
    let insetPoints: CGFloat = -1.0
    let insetUOffset = insetPoints / W
    let insetVOffset = insetPoints / H
    let usableUNorm = (W - insetPoints * 2) / W
    let usableVNorm = (H - insetPoints * 2) / H

    let transform = MeshTransform()
    for v in template.vertices {
        let worldX = v.baseX + v.sizeScaleX * W
        let worldY = v.baseY + v.sizeScaleY * H
        let u = worldX / W
        let vCoord = worldY / H
        let mappedU = insetUOffset + u * usableUNorm
        let mappedV = insetVOffset + vCoord * usableVNorm
        let fromX = max(0.0, min(1.0, mappedU + v.dispX * displacementMagnitudeU))
        let fromY = max(0.0, min(1.0, mappedV + v.dispY * displacementMagnitudeV))
        transform.add(MeshTransform.Vertex(
            from: CGPoint(x: fromX, y: fromY),
            to: MeshTransform.Point3D(x: mappedU, y: mappedV, z: v.depth)
        ))
    }
    for face in template.faces {
        transform.add(face)
    }
    return transform
}

func generateGlassMeshTemplate(
    cornerRadius: CGFloat,
    edgeDistance: CGFloat,
    cornerResolution: Int,
    outerEdgeDistance: CGFloat,
    bezier: DisplacementBezier
) -> GlassMeshTemplate {
    let clampedRadius = cornerRadius

    // Reference size for displacement computation (must be >= 2R per axis)
    let refW = max(4 * clampedRadius, 100)
    let refH = max(4 * clampedRadius, 100)

    var vertices = ContiguousArray<GlassMeshTemplate.VertexTemplate>()
    var faces = ContiguousArray<MeshTransform.Face>()
    var vertexIndex: Int = 0

    // Compute unitless displacement (direction * weight * bezier * edgeBoost) at reference size
    func templateDisplacement(worldX: CGFloat, worldY: CGFloat) -> (CGFloat, CGFloat) {
        let (rawDispX, rawDispY, sdf) = computeDisplacement(
            x: worldX, y: worldY,
            width: refW, height: refH,
            cornerRadius: clampedRadius,
            edgeDistance: edgeDistance,
            bezier: bezier
        )
        let distToEdge = max(0.0, -sdf)
        let edgeBand = max(0.0, outerEdgeDistance)
        let edgeBoost: CGFloat
        if edgeBand > 0 {
            let t = max(0.0, min(1.0, (edgeBand - distToEdge) / edgeBand))
            edgeBoost = 1.0 + t * t * (3 - 2 * t) * 0.5
        } else {
            edgeBoost = 1.0
        }
        return (rawDispX * edgeBoost, rawDispY * edgeBoost)
    }

    func addVertex(baseX: CGFloat, scaleX: CGFloat, baseY: CGFloat, scaleY: CGFloat, depth: CGFloat = 0) -> Int {
        let worldX = baseX + scaleX * refW
        let worldY = baseY + scaleY * refH
        let (dispX, dispY) = templateDisplacement(worldX: worldX, worldY: worldY)
        vertices.append(GlassMeshTemplate.VertexTemplate(
            baseX: baseX, sizeScaleX: scaleX,
            baseY: baseY, sizeScaleY: scaleY,
            dispX: dispX, dispY: dispY, depth: depth
        ))
        let idx = vertexIndex
        vertexIndex += 1
        return idx
    }

    func addQuadFace(_ i0: Int, _ i1: Int, _ i2: Int, _ i3: Int) {
        faces.append(MeshTransform.Face(
            indices: (UInt32(i0), UInt32(i1), UInt32(i2), UInt32(i3)),
            w: (0.0, 0.0, 0.0, 0.0)
        ))
    }

    // Topology parameters (same formulas as generateGlassMesh)
    let angularStepsBase = max(3, cornerResolution)
    let angularSteps = angularStepsBase % 2 == 0 ? angularStepsBase : angularStepsBase + 1
    let radialSteps = max(2, cornerResolution)
    let horizontalSegments = max(2, cornerResolution / 2 + 1)
    let verticalSegments = max(2, cornerResolution / 2 + 1)
    let R = clampedRadius

    func depthFactorsWithOuterBand(count: Int, band: CGFloat, maxRadius: CGFloat) -> [CGFloat] {
        guard count > 0, maxRadius > 0 else { return [0, 1] }
        let bandNorm = max(0, min(1, band / maxRadius))
        let innerSegments = max(1, count - 1)
        let innerMax = max(0, 1 - bandNorm)
        var factors: [CGFloat] = (0...innerSegments).map { i in
            innerMax * CGFloat(i) / CGFloat(innerSegments)
        }
        func appendUnique(_ value: CGFloat) {
            if let last = factors.last, abs(last - value) < 1e-4 { return }
            factors.append(value)
        }
        appendUnique(innerMax)
        appendUnique(1.0)
        return factors
    }

    let depthFactors = depthFactorsWithOuterBand(count: radialSteps, band: outerEdgeDistance, maxRadius: R)
    let angularFactors = (0...angularSteps).map { CGFloat($0) / CGFloat(angularSteps) }
    let outerToInner = depthFactors.reversed()

    // Affine coefficient arrays for strip/grid positions
    let topXCoeffs: [(base: CGFloat, scale: CGFloat)] = (0...horizontalSegments).map { i in
        let t = CGFloat(i) / CGFloat(horizontalSegments)
        return (base: R * (1 - 2 * t), scale: t)
    }
    let sideYCoeffs: [(base: CGFloat, scale: CGFloat)] = (0...verticalSegments).map { j in
        let t = CGFloat(j) / CGFloat(verticalSegments)
        return (base: R * (1 - 2 * t), scale: t)
    }
    let topYCoeffs: [(base: CGFloat, scale: CGFloat)] = outerToInner.map { factor in
        (base: R * (1 - factor), scale: 0)
    }
    let bottomYCoeffs: [(base: CGFloat, scale: CGFloat)] = depthFactors.map { factor in
        (base: -R * (1 - factor), scale: 1)
    }
    let leftXCoeffs: [(base: CGFloat, scale: CGFloat)] = outerToInner.map { factor in
        (base: R * (1 - factor), scale: 0)
    }
    let rightXCoeffs: [(base: CGFloat, scale: CGFloat)] = depthFactors.map { factor in
        (base: -R * (1 - factor), scale: 1)
    }

    // Build a grid of vertices from coefficient arrays and emit quad faces
    func buildGridTemplate(
        xCoeffs: [(base: CGFloat, scale: CGFloat)],
        yCoeffs: [(base: CGFloat, scale: CGFloat)]
    ) {
        var indexGrid: [[Int]] = []
        for yc in yCoeffs {
            var row: [Int] = []
            for xc in xCoeffs {
                row.append(addVertex(baseX: xc.base, scaleX: xc.scale, baseY: yc.base, scaleY: yc.scale))
            }
            indexGrid.append(row)
        }
        let numRows = indexGrid.count - 1
        let numCols = indexGrid.first!.count - 1
        for row in 0..<numRows {
            for col in 0..<numCols {
                addQuadFace(
                    indexGrid[row][col],
                    indexGrid[row][col + 1],
                    indexGrid[row + 1][col + 1],
                    indexGrid[row + 1][col]
                )
            }
        }
    }

    // Corner wedge template
    func buildCornerTemplate(
        centerBaseX: CGFloat, centerScaleX: CGFloat,
        centerBaseY: CGFloat, centerScaleY: CGFloat,
        startAngle: CGFloat, endAngle: CGFloat
    ) {
        let ringRadials = outerToInner.filter { $0 > 0 }
        guard !ringRadials.isEmpty else { return }

        var ringIndices: [[Int]] = []
        for radial in ringRadials {
            let r = R * radial
            var row: [Int] = []
            for t in angularFactors {
                let angle = startAngle + (endAngle - startAngle) * t
                let offsetX = r * cos(angle)
                let offsetY = r * sin(angle)
                row.append(addVertex(
                    baseX: centerBaseX + offsetX, scaleX: centerScaleX,
                    baseY: centerBaseY + offsetY, scaleY: centerScaleY
                ))
            }
            ringIndices.append(row)
        }

        // Quad rings between concentric samples
        for r in 0..<(ringIndices.count - 1) {
            let outerRing = ringIndices[r]
            let innerRing = ringIndices[r + 1]
            for i in 0..<(outerRing.count - 1) {
                addQuadFace(outerRing[i], outerRing[i + 1], innerRing[i + 1], innerRing[i])
            }
        }

        // Center fan collapse (same logic as original)
        if let innermostRing = ringIndices.last {
            let ringSegments = innermostRing.count - 1
            guard ringSegments >= 2 else { return }

            let centerAnchor = addVertex(
                baseX: centerBaseX, scaleX: centerScaleX,
                baseY: centerBaseY, scaleY: centerScaleY,
                depth: -0.02
            )
            let stride = 2
            var i = 0
            while i + 2 <= ringSegments {
                addQuadFace(centerAnchor, innermostRing[i], innermostRing[i + 1], innermostRing[i + 2])
                i += stride
            }
            if i < ringSegments {
                addQuadFace(centerAnchor, innermostRing[ringSegments - 1], innermostRing[ringSegments], innermostRing[ringSegments])
            }
        }
    }

    // Edge strips
    buildGridTemplate(xCoeffs: topXCoeffs, yCoeffs: topYCoeffs)
    buildGridTemplate(xCoeffs: topXCoeffs, yCoeffs: bottomYCoeffs)
    buildGridTemplate(xCoeffs: leftXCoeffs, yCoeffs: sideYCoeffs)
    buildGridTemplate(xCoeffs: rightXCoeffs, yCoeffs: sideYCoeffs)

    // Center patch
    buildGridTemplate(xCoeffs: topXCoeffs, yCoeffs: sideYCoeffs)

    // Corners
    buildCornerTemplate(
        centerBaseX: R, centerScaleX: 0,
        centerBaseY: R, centerScaleY: 0,
        startAngle: .pi, endAngle: 1.5 * .pi
    )
    buildCornerTemplate(
        centerBaseX: -R, centerScaleX: 1,
        centerBaseY: R, centerScaleY: 0,
        startAngle: 1.5 * .pi, endAngle: 2 * .pi
    )
    buildCornerTemplate(
        centerBaseX: -R, centerScaleX: 1,
        centerBaseY: -R, centerScaleY: 1,
        startAngle: .pi / 2, endAngle: 0
    )
    buildCornerTemplate(
        centerBaseX: R, centerScaleX: 0,
        centerBaseY: -R, centerScaleY: 1,
        startAngle: .pi, endAngle: .pi / 2
    )

    return GlassMeshTemplate(vertices: vertices, faces: faces)
}

/// Computes signed distance from a point to the edge of a rounded rectangle.
/// Returns negative inside, zero on edge, positive outside.
/// All values in points.
