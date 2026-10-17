#!/usr/bin/env python3
"""Run the original Swift rig to produce independent Android geometry test vectors."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--verify-ref", help="Compile the Swift art at this Git ref and require exact equality with the committed corpus")
parser.add_argument("--equivalence-output", type=Path, help="Save verified source variants without regenerating the reference art")
args = parser.parse_args()
if args.equivalence_output and not args.verify_ref:
    parser.error("--equivalence-output requires --verify-ref")


def source_bytes(path):
    if args.verify_ref:
        return subprocess.run(["git", "show", args.verify_ref + ":" + str(path.relative_to(root))],
                              cwd=root, check=True, capture_output=True).stdout
    return path.read_bytes()


hand = root / "apple/Shared/DesignSystem/HandDrawn.swift"
mascot = root / "apple/Shared/DesignSystem/Mascot.swift"
hand_source = source_bytes(hand).decode()
mascot_source = source_bytes(mascot).decode()

# Keep the original equations verbatim. Only the SwiftUI color/path value types
# are replaced to let this reference run as a standalone macOS Swift program.
support = r"""
import Foundation
import CoreGraphics
struct Color {
    var rgb: Int
    var alpha: Double = 1
    init(hex: String) { rgb = Int(hex.dropFirst(), radix: 16)! }
    init(rgb: Int, alpha: Double = 1) { self.rgb = rgb; self.alpha = alpha }
    func opacity(_ value: Double) -> Color { Color(rgb: rgb, alpha: alpha * value) }
    static let clear = Color(rgb: 0, alpha: 0)
    static let white = Color(rgb: 0xffffff)
}
struct Path {
    var points: [CGPoint] = []
    mutating func addLines(_ value: [CGPoint]) { points += value }
    mutating func closeSubpath() {}
}
"""
types = hand_source[hand_source.index("struct InkRNG {"):hand_source.index("enum Ink {")]
geometry = "enum Ink {\n" + hand_source[hand_source.index("    static func catmull("):hand_source.index("/// `-RodaFreezeArt")]
rig = mascot_source[mascot_source.index("enum MascotPose:"):mascot_source.index("struct MascotView:")]
doodles = hand_source[hand_source.index("enum Doodle {"):hand_source.index("struct DoodleView:")]
export = r"""
func point(_ p: CGPoint) -> [Double] { [Double(p.x), Double(p.y)] }
func color(_ c: Color?) -> Any { c.map { [$0.rgb, $0.alpha] as [Any] } ?? NSNull() }
func stroke(_ s: InkStroke) -> [String: Any] {
    ["points": s.points.map(point), "closed": s.closed, "width": Double(s.width),
     "color": color(s.color), "fill": color(s.fill), "start": s.start, "span": s.span,
     "wobble": Double(s.wobble), "smooth": s.smooth, "opacity": s.opacity,
     "ghost": s.ghost, "misregister": Double(s.misregister)]
}
var frames: [[String: Any]] = []
// Includes draw-on, blink, turn/glance, and timestamps past the former 8-second reset.
let times = [0.0, 0.37, 2.3, 3.78, 8.001, 10.3, 63.25]
for name in MascotPose.allCases.map({ $0.rawValue }) + ["head", "head_smirk", "head_working"] {
    for t in times {
        let s: [InkStroke]
        if name == "head_working" { s = Mascot.workingHead(t: t) }
        else if name.hasPrefix("head") { s = Mascot.head(t: t, mood: name == "head_smirk" ? .smirk : .pout) }
        else { s = Mascot.strokes(MascotPose(rawValue: name)!, t: t) }
        frames.append(["name": name, "time": t, "strokes": s.map(stroke)])
    }
}
let p = [CGPoint(x: 0.13, y: 0.28), CGPoint(x: 0.3, y: 0.81), CGPoint(x: 0.88, y: 0.31), CGPoint(x: 0.49, y: 0.12)]
var rng = InkRNG(78)
let catmull = Ink.catmull(p, closed: false)
let trimmed = Ink.trim(catmull, 0.37)
let rendered = Ink.ribbon(trimmed, width: 0.016, tapered: true, rng: &rng)
let geometry: [String: Any] = ["points": p.map(point), "catmull": catmull.map(point),
    "closed_catmull": Ink.catmull(p, closed: true).map(point),
    "linear": Ink.linear(p, closed: true).map(point), "trimmed": trimmed.map(point), "ribbon": rendered.points.map(point)]
var seed = InkRNG(0xffffffffffffffff)
var doodleFrames: [[String: Any]] = []
for (name, doodle) in [("pot", Doodle.pot), ("ballot", Doodle.ballot), ("notepad", Doodle.notepad), ("trip", Doodle.trip), ("hike", Doodle.hike)] {
    for t in times + [2.999, 3.001, 4.0] {
        doodleFrames.append(["name": name, "time": t, "seed": doodle.seed, "strokes": doodle.strokes(t).map(stroke)])
    }
}
let document: [String: Any] = ["frames": frames, "doodles": doodleFrames, "geometry": geometry, "rng": (0..<16).map { _ in seed.unit() }]
let data = try JSONSerialization.data(withJSONObject: document, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
"""
with tempfile.TemporaryDirectory(prefix="zoen-ink-reference-") as directory:
    source = Path(directory) / "reference.swift"
    executable = Path(directory) / "reference"
    source.write_text(support + types + geometry + rig + doodles + export)
    subprocess.run(["swiftc", "-O", str(source), "-o", str(executable)], check=True)
    result = subprocess.run([str(executable)], check=True, capture_output=True)

document = json.loads(result.stdout)
target = root / "android/app/src/test/resources/ink-swift-reference.json.gz"
if args.verify_ref:
    reference = json.loads(gzip.decompress(target.read_bytes()))
    corpus = {key: value for key, value in reference.items() if key != "sources"}
    if corpus != document:
        raise SystemExit("The newer Swift source changes the original mascot, doodle or geometry corpus; adapt Android before accepting it")
    verified = {
        "verified_ref": subprocess.run(["git", "rev-parse", args.verify_ref], cwd=root, check=True, capture_output=True, text=True).stdout.strip(),
        "reference_corpus_sha256": hashlib.sha256(json.dumps(document, separators=(",", ":"), sort_keys=True).encode()).hexdigest(),
        "mascot_frames_compared": len(document["frames"]),
        "doodle_frames_compared": len(document["doodles"]),
        "comparison": "Exact independently compiled Swift mascot and doodle strokes, curve/trim/ribbon geometry and RNG samples; existing corpus unchanged",
        "source_variants": {str(p.relative_to(root)): sorted({reference["sources"][str(p.relative_to(root))], hashlib.sha256(source_bytes(p)).hexdigest()}) for p in (hand, mascot)},
    }
    if args.equivalence_output:
        if args.equivalence_output.exists():
            previous = json.loads(args.equivalence_output.read_text())
            if previous["reference_corpus_sha256"] != verified["reference_corpus_sha256"]:
                raise SystemExit("Existing source equivalence belongs to a different art corpus")
            for path, variants in verified["source_variants"].items():
                verified["source_variants"][path] = sorted(set(variants) | set(previous["source_variants"].get(path, [])))
        args.equivalence_output.write_text(json.dumps(verified, indent=2) + "\n")
    print(json.dumps(verified, indent=2))
    raise SystemExit(0)

document["sources"] = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in (hand, mascot)}
target.parent.mkdir(parents=True, exist_ok=True)
target.write_bytes(gzip.compress(json.dumps(document, separators=(",", ":"), sort_keys=True).encode(), mtime=0))
print(f"{len(document['frames'])} mascot and {len(document['doodles'])} doodle frames from original Swift; {target.stat().st_size} compressed bytes; {target.relative_to(root)}")
