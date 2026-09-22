#!/usr/bin/env python3
"""Cut fresh native recordings on beats 1 and 3 of a 120 BPM grid.

Each JSON shot describes a real source range, editorial camera framing and copy.
No application controls or artwork are synthesized. Output is a silent 60 fps
MP4/WebM, poster, WebVTT description track and a reproducible edit manifest.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess

FPS, WIDTH, HEIGHT = 60, 1920, 1080


def run(args):
    subprocess.run(args, check=True)


def probe(path):
    return json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)
    ]))


def filter_path(path):
    return "'" + str(path.resolve()).replace("\\", "\\\\").replace(":", "\\:").replace("'", "'\\''") + "'"


def caption_filter(work, index, text):
    text_path = work / f"copy-{index:02}.txt"
    text_path.write_text(text)
    font = Path("/usr/share/fonts/liberation/LiberationSans-Bold.ttf")
    # Copy is an editorial overlay. The camera's focal region remains unobscured.
    return (
        "drawbox=x=0:y=944:w=iw:h=136:color=0x10111a@0.89:t=fill,"
        "drawbox=x=64:y=978:w=5:h=54:color=0xA6E3A1:t=fill,"
        f"drawtext=fontfile={filter_path(font)}:textfile={filter_path(text_path)}"
        ":expansion=none:fontsize=50:fontcolor=0xF4F5FC:x=92:y=980"
    )


def encode_options(crf):
    return ["-an", "-c:v", "libx264", "-preset", "medium", "-crf", str(crf),
            "-threads", "4", "-pix_fmt", "yuv420p", "-r", str(FPS),
            "-g", str(FPS), "-keyint_min", str(FPS), "-sc_threshold", "0",
            "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
            "-video_track_timescale", "15360", "-movflags", "+faststart"]


def render_shot(index, shot, source, work, crf):
    output = work / f"shot-{index:02}.mp4"
    path = source / shot["source"]
    start, end = shot["in"], shot["out"]
    span = end - start
    if span <= 0 or not all(math.isfinite(v) for v in [start, end]):
        raise ValueError(f"Invalid source window in shot {index}")
    info = probe(path)
    if end > float(info["format"]["duration"]) + .001:
        raise ValueError(f"Shot {index} extends beyond {path.name}")
    fault_path = source / (path.stem + "-errors.json")
    if fault_path.exists():
        capture = json.loads((source / (path.stem + "-capture.json")).read_text())
        for fault in json.loads(fault_path.read_text()):
            frame = re.search(r"frame (\d+):", fault)
            if frame is None or start <= int(frame[1])/capture["fps"] <= end:
                raise ValueError(f"Unresolved action in selected shot {index}: {fault}")
        # A longer take can contain unused actions outside these source windows.
        # They are retained in the manifest; each selected outcome is visually
        # checked in addition to rejecting failures inside the included window.
    z0, z1 = shot.get("zoom", [1.0, 1.08])
    x0, x1 = shot.get("x", [.5, .5])
    y0, y1 = shot.get("y", [.5, .5])
    if min(z0, z1) < 1 or max(z0, z1) > 2.2:
        raise ValueError("Editorial zoom must remain in [1, 2.2]")
    # Settle on the cut, travel quickly between beats, settle into the next cut.
    ease = "(0.5-0.5*cos(PI*min(on/59,1)))"
    zoom = f"{z0}+({z1-z0})*{ease}"
    x = f"(iw-iw/zoom)*({x0}+({x1-x0})*{ease})"
    y = f"(ih-ih/zoom)*({y0}+({y1-y0})*{ease})"
    normalized = f"((T-STARTT)/{span})"
    timing = f"({normalized}+0.075*sin(2*PI*{normalized}))/TB"
    filters = [
        f"trim=start={start}:end={end}", f"setpts='{timing}'", f"fps={FPS}",
        "tpad=stop_mode=clone:stop_duration=0.1", "trim=duration=1",
        "scale=2400:1350:flags=lanczos", "setsar=1",
        f"zoompan=z='{zoom}':x='{x}':y='{y}':d=1:s={WIDTH}x{HEIGHT}:fps={FPS}",
    ]
    if shot.get("copy"):
        filters.append(caption_filter(work, index, shot["copy"]))
    filters.extend(["scale=out_color_matrix=bt709:out_range=tv", "format=yuv420p"])
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
         "-i", str(path), "-vf", ",".join(filters), *encode_options(crf),
         "-frames:v", str(FPS), str(output)])
    check = probe(output)
    stream = next(s for s in check["streams"] if s["codec_type"] == "video")
    assert int(stream["nb_frames"]) == FPS
    return output


def timestamp(second):
    return f"00:{second//60:02}:{second%60:02}.000"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--crf", type=int, default=18)
    parser.add_argument("--shot", type=int, help="Render only one shot for framing review")
    parser.add_argument("--reuse-shots", action="store_true", help="Reuse reviewed segment files")
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    shots = plan["shots"]
    assert plan["bpm"] == 120 and plan["cut_beats"] == [1, 3]
    assert len(shots) % 2 == 0, "End on a complete bar"
    args.work.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.shot is not None:
        print(render_shot(args.shot, shots[args.shot], args.sources, args.work, args.crf))
        return

    def render(item):
        index, shot = item
        path = args.work / f"shot-{index:02}.mp4"
        if args.reuse_shots and path.exists():
            return path
        return render_shot(index, shot, args.sources, args.work, args.crf)

    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        segments = list(executor.map(render, enumerate(shots)))
    concat = args.work / "concat.txt"
    concat.write_text("".join(f"file '{p.resolve()}'\n" for p in segments))
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
         "-f", "concat", "-safe", "0", "-i", str(concat), "-map", "0:v:0",
         "-c", "copy", "-an", "-movflags", "+faststart", str(args.output)])
    info = probe(args.output)
    stream = next(s for s in info["streams"] if s["codec_type"] == "video")
    assert int(stream["nb_frames"]) == len(shots)*FPS
    assert abs(float(info["format"]["duration"])-len(shots)) < 1/FPS
    assert not any(s["codec_type"] == "audio" for s in info["streams"])
    run(["ffmpeg", "-v", "error", "-i", str(args.output), "-f", "null", "-"])
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
         "-i", str(args.output), "-an", "-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0",
         "-deadline", "good", "-cpu-used", "4", "-row-mt", "1", "-threads", "6",
         str(args.output.with_suffix(".webm"))])
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
         "-ss", str(plan.get("poster_at", .4)), "-i", str(args.output), "-frames:v", "1",
         "-c:v", "libwebp", "-quality", "90", str(args.output.with_suffix(".webp"))])
    captions = ["WEBVTT", ""]
    cues = plan.get("captions", [
        {"start": i, "end": i+1, "text": shot["description"]}
        for i, shot in enumerate(shots)
    ])
    for cue in cues:
        captions += [f"{timestamp(cue['start'])} --> {timestamp(cue['end'])}", cue["text"], ""]
    args.output.with_suffix(".vtt").write_text("\n".join(captions))
    manifest = {**plan, "fps": FPS, "width": WIDTH, "height": HEIGHT,
                "seconds": len(shots), "frames": len(shots)*FPS, "audio": False,
                "cut_frames": list(range(0, len(shots)*FPS, FPS)),
                "provenance": "Fresh native Omadesign WGPU recordings driven through the actual app UI; editorial speed ramps, camera crops and captions. The two-second brand ending uses the official SVG. No simulated UI frames.",
                "raw_take_action_reports": {name: json.loads((args.sources/(Path(name).stem+'-errors.json')).read_text())
                                            for name in sorted({s["source"] for s in shots})
                                            if (args.sources/(Path(name).stem+'-errors.json')).exists()},
                "source_sha256": {name: hashlib.sha256((args.sources/name).read_bytes()).hexdigest()
                                  for name in sorted({s["source"] for s in shots})},
                "outputs": {p.name: {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
                            for p in [args.output.with_suffix(s) for s in [".mp4", ".webm", ".webp", ".vtt"]]}}
    args.output.with_suffix(".json").write_text(json.dumps(manifest, indent=2)+"\n")
    print(json.dumps({"file": str(args.output), "seconds": len(shots), "frames": len(shots)*FPS}))


if __name__ == "__main__":
    main()
