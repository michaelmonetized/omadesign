#!/usr/bin/env python3
"""Validate isolated native replay output and prepare the website's video assets.

Run after `cargo run --release --bin capture_studios -- SCENE OUTPUT_DIR`.
Original documents stay untouched. This script only copies verified MP4s and
encodes posters; it does not invent application frames or overlay UI.
"""
import argparse
import hashlib
import json
from datetime import date
import tomllib
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SCENES = {
    "graphics": (17, 3, [(0, "Gradient placement", "Drag the native canvas handles to place and reshape the fill gradient."), (7, "Gradient direction", "Drag a new direction for the selected gradient."), (11, "Group and undo", "Group artwork, ungroup, undo and redo.")]),
    "chroma": (15, 3, [(0, "Compare and sample", "Compare original and preview, then sample the key color."), (6, "Apply and undo", "Apply the key, undo and redo."), (11, "Reopen preview", "Reopen the chroma key preview and cancel.")]),
    "layout": (15, 3, [(0, "Present", "Open the native interactive Layout presentation."), (7, "Draw a frame", "Use the Frame tool to draw a new frame."), (12, "Undo", "Undo the new frame in the native workspace.")]),
    "design": (42, 4, [
        (0, "Layout", "Rotate the selected shape and round its corners with the Layout controls."),
        (6, "Appearance", "Add a stroke, inspect stroke options, and change the fill to a gradient."),
        (12, "Typography", "Adjust the headline spacing and explore OpenType features."),
        (18, "Reshape", "Enter Warp mesh and drag a real cage handle to reshape the artwork."),
        (24, "Effects", "Add a drop shadow in the native Effects panel."),
        (30, "Trace", "Inspect tracing options and the Ignore white setting."),
        (36, "Layers", "Compare Multiply and Normal blend modes on the artwork layer."),
    ]),
    "pixel": (30, 4, [
        (0, "Brush", "Adjust the brush size and paint a highlight on a separate layer."),
        (6, "Retouch", "Choose the Healing brush, Alt-click a source, and blend a short stroke into the painting."),
        (12, "Mask", "Add a layer mask and brush away part of the new highlight."),
        (18, "Color", "Inspect the color picker and choose a project palette color for the brush."),
        (24, "Layers", "Compare layer blending and use Undo on the actual document."),
    ]),
    "photo": (24, 10, [
        (0, "Light", "Adjust exposure and shadows, then compare the original with Before."),
        (6, "Color", "Warm the scene slightly and adjust vibrance in the Color panel."),
        (12, "Detail", "Add a gentle clarity and vignette adjustment."),
        (18, "Compare", "Return to Light controls and compare the developed image with Before."),
    ]),
    "motion": (30, 3, [
        (0, "Presets", "Apply Draw stroke to the selected rings and play the actual animation."),
        (6, "Keyframes", "Add a key with the native keyboard shortcut and undo it."),
        (12, "Pop in", "Apply Pop in and play the resulting animation."),
        (18, "Slide up", "Apply Slide up and play the resulting animation."),
        (24, "Layers", "Compare blend modes and play the edited animation."),
    ]),
    "brand-kit": (30, 8, [
        (0, "Palettes", "Choose colors from two project palettes and apply them to the selected shape."),
        (10, "Brand assets", "Filter the project asset bank, drag an SVG onto the artboard, and undo its placement."),
        (20, "Typography", "Apply project Heading and Body font roles to the selected headline."),
    ]),
}

def timestamp(seconds):
    return f"00:{int(seconds) // 60:02}:{int(seconds) % 60:02}.000"

def run(*args):
    return subprocess.check_output(args, text=True)

def publish(source, destination, names):
    destination.mkdir(parents=True, exist_ok=True)
    for name in names:
        seconds, poster_at, chapters = SCENES[name]
        video = source / f"{name}.mp4"
        faults = json.loads((source / f"{name}-errors.json").read_text())
        if faults:
            raise SystemExit(f"Refusing unresolved replay targets in {name}: {faults}")
        info = json.loads(run("ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(video)))
        stream = next(s for s in info["streams"] if s["codec_type"] == "video")
        if stream["codec_name"] != "h264" or stream["width"] != 1600 or stream["height"] != 900:
            raise SystemExit(f"Unexpected native video format: {name}")
        if abs(float(info["format"]["duration"]) - seconds) > .05 or int(stream["nb_frames"]) != seconds * 30:
            raise SystemExit(f"Unexpected replay timing: {name}")
        shutil.copy2(video, destination / video.name)
        # Chromium builds on Linux may omit H.264. Supply an open-codec source too.
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(video),
                        "-an", "-c:v", "libvpx-vp9", "-crf", "34", "-b:v", "0",
                        "-deadline", "realtime", "-cpu-used", "6", "-row-mt", "1",
                        str(destination / f"{name}.webm")], check=True)
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", str(poster_at), "-i", str(video), "-frames:v", "1", "-c:v", "libwebp", "-quality", "86", str(destination / f"{name}.webp")], check=True)
        cues = ["WEBVTT", ""]
        for i, (start, title, text) in enumerate(chapters):
            end = chapters[i + 1][0] if i + 1 < len(chapters) else seconds
            cues.extend([f"{timestamp(start)} --> {timestamp(end)}", f"{title}: {text}", ""])
        (destination / f"{name}.vtt").write_text("\n".join(cues), encoding="utf-8")
    assets = []
    for name in names:
        seconds, _, chapters = SCENES[name]
        video = destination / f"{name}.mp4"
        if not video.exists():
            continue
        assets.append({
            "id": name, "duration": seconds, "width": 1600, "height": 900, "fps": 30,
            "webm_sha256": hashlib.sha256((destination / f"{name}.webm").read_bytes()).hexdigest(),
            "webm_bytes": (destination / f"{name}.webm").stat().st_size,
            "src": f"media/recordings/{name}.mp4", "webm": f"media/recordings/{name}.webm" if (destination / f"{name}.webm").exists() else None, "poster": f"media/recordings/{name}.webp",
            "captions": f"media/recordings/{name}.vtt", "bytes": video.stat().st_size,
            "sha256": hashlib.sha256(video.read_bytes()).hexdigest(),
            "chapters": [{"start": start, "label": title, "description": text} for start, title, text in chapters],
        })
    (destination / "manifest.json").write_text(json.dumps({
        "version": 1,
        "captured": date.today().isoformat(),
        "capture_version": tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"],
        "capture_binary_sha256": hashlib.sha256((ROOT / "target/release/capture_studios").read_bytes()).hexdigest(),
        "provenance": "Native omadesign WGPU viewport recordings. Seeded example documents; actual egui pointer and keyboard input during replay. The only overlay is a pointer indicator. No audio.",
        "recordings": assets,
    }, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({a["id"]: {"seconds": a["duration"], "bytes": a["bytes"]} for a in assets}, indent=2))

if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source", type=Path)
    p.add_argument("scenes", nargs="*", choices=list(SCENES), default=list(SCENES))
    p.add_argument("--output", type=Path, default=ROOT / "site/public/media/recordings")
    args = p.parse_args()
    publish(args.source, args.output, args.scenes)
