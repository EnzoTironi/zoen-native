from pathlib import Path
import argparse, subprocess, shutil, hashlib, json

parser = argparse.ArgumentParser(description="Encode original SwiftUI mascot exports as visual review references.")
parser.add_argument("frames", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
out = root / "build/android-evidence/reference-raster"
out.mkdir(parents=True, exist_ok=True)
assets = []
for pose in ["wave", "phone", "map", "run", "juggle", "walk", "cheer", "head", "head_smirk", "head_working"]:
    source = args.frames / pose
    assert len(list(source.glob("*.png"))) == 96, source
    still = out / f"zoen_mascot_{pose}.png"
    motion = out / f"zoen_mascot_{pose}_motion.webp"
    shutil.copy2(source / "000.png", still)
    command = ["img2webp", "-loop", "0", "-lossless", "-m", "6"]
    for frame in range(96):
        duration = round((frame + 1) * 1000 / 12) - round(frame * 1000 / 12)
        command += ["-d", str(duration), str(source / f"{frame:03d}.png")]
    subprocess.run(command + ["-o", str(motion)], check=True)
    for file in (still, motion):
        assets.append({"file": str(file.relative_to(root)), "bytes": file.stat().st_size, "sha256": hashlib.sha256(file.read_bytes()).hexdigest()})
    print(pose, motion.stat().st_size, flush=True)
manifest = {
    "renderer": "Original SwiftUI Mascot + Ink Canvas renderer",
    "source_files": {str(p): hashlib.sha256((root / p).read_bytes()).hexdigest() for p in [Path("apple/Shared/DesignSystem/Mascot.swift"), Path("apple/Shared/DesignSystem/HandDrawn.swift")]},
    "size": {"body": 384, "head": 128}, "frames_per_pose": 96, "fps": 12,
    "start_seconds": 2.3, "loop_seconds": 8, "alpha": True,
    "encoding": "lossless animated WebP; first frame PNG for reduced motion", "assets": assets,
}
(out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
