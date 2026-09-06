#!/usr/bin/env python3
"""Recut the native studio demo with the 30-second logo construction and final logo.

Run --plan to review timing and copy without reading captures or encoding anything.
Inputs are native capture clips; output is a chaptered H.264/AAC film.
Requires ffmpeg with drawtext/libx264 and ffprobe; uses installed Noto Sans fonts.
"""
from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
from dataclasses import dataclass


WIDTH, HEIGHT, FPS = 1920, 1080, 30
APP_W, APP_H, APP_X, APP_Y = 1712, 963, 104, 104
BACKGROUND = "0x111317"
PRIMARY = "0xe9e9e8"
SECONDARY = "0xa2a5ae"
CORAL = "0xe88778"
INTRO_SECONDS, OUTRO_SECONDS = 2, 5
TOTAL_SECONDS = 97


@dataclass(frozen=True)
class Chapter:
    filename: str
    seconds: int
    title: str
    subtitle: str


CHAPTERS = (
    Chapter("00-welcome", 2, "A calmer place to start.",
            "A clearer welcome, with useful starting points close at hand."),
    Chapter("01-templates", 4, "52 good starts.",
            "Browse the editable template library and choose a direction."),
    Chapter("02-reflow", 5, "Make room for the idea.",
            "Choose a size. Let the layout adjust with it."),
    Chapter("03-logo", 30, "A mark of our own.",
            "From the Omarchy maze to six precise pen-drawn letters, then a little motion."),
    Chapter("04-precision", 7, "Find the right place.",
            "Guides, ruler controls, 45° constraints and spacing-aware snapping."),
    Chapter("05-deform", 6, "A little bend.",
            "Distort, skew, perspective and a live vector warp mesh."),
    Chapter("06-paths", 6, "Paths with possibilities.",
            "Stroke outlines, Select Same and Pathfinder operations."),
    Chapter("07-masking", 4, "Hide. Reveal. Keep the original.",
            "Editable layer masks keep the original artwork intact."),
    Chapter("08-healing", 4, "A softer touch.",
            "Sample clean texture and blend away small distractions."),
    Chapter("09-photo", 6, "A fresh point of view.",
            "Develop, compare and crop photos in the same studio."),
    Chapter("10-motion", 8, "Give it a little life.",
            "13 motion presets, built from editable animation keys."),
    Chapter("11-hud", 8, "The right key, right here.",
            "Shortcut hints follow your tool, selection and held modifiers."),
)
assert sum(chapter.seconds for chapter in CHAPTERS) == 90
assert INTRO_SECONDS + sum(c.seconds for c in CHAPTERS) + OUTRO_SECONDS == TOTAL_SECONDS
assert APP_X + APP_W <= WIDTH and APP_Y + APP_H <= HEIGHT


def run(arguments: list[str]) -> None:
    subprocess.run(arguments, check=True)


def probe(path: Path, ffprobe: str) -> dict:
    completed = subprocess.run(
        [ffprobe, "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
        check=True, capture_output=True, text=True,
    )
    return json.loads(completed.stdout)


def duration(info: dict, stream: dict | None = None) -> float:
    value = (stream or {}).get("duration", info.get("format", {}).get("duration"))
    result = float(value)
    if not math.isfinite(result):
        raise ValueError("Media has no finite duration")
    return result


def filter_path(path: Path) -> str:
    # Values are passed directly to ffmpeg, never through a shell. Filtergraph
    # paths still need their own quoting (including the drawtext option colon).
    value = str(path.resolve()).replace("\\", "\\\\").replace(":", "\\:")
    value = value.replace("'", "'\\''")
    return "'" + value + "'"


def find_font(weight: str) -> Path:
    choices = {
        "regular": ["/usr/share/fonts/noto/NotoSans-Regular.ttf",
                    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf"],
        "medium": ["/usr/share/fonts/noto/NotoSans-Medium.ttf",
                   "/usr/share/fonts/liberation/LiberationSans-Bold.ttf"],
    }
    for name in choices[weight]:
        if Path(name).is_file():
            return Path(name)
    raise FileNotFoundError(f"No installed {weight} font found; update find_font() before rendering")


def text_filter(work: Path, slug: str, content: str, font: Path, size: int,
                color: str, x: str | int, y: int) -> str:
    text_path = work / f"{slug}.txt"
    text_path.write_text(content, encoding="utf-8")
    return (f"drawtext=fontfile={filter_path(font)}:textfile={filter_path(text_path)}"
            f":expansion=none:fontsize={size}:fontcolor={color}:x={x}:y={y}")


def encode_options(crf: int, preset: str, seconds: int) -> list[str]:
    return ["-an", "-c:v", "libx264", "-preset", preset, "-crf", str(crf),
            "-pix_fmt", "yuv420p", "-r", str(FPS), "-frames:v", str(seconds * FPS),
            "-profile:v", "high", "-level:v", "4.1", "-g", str(FPS * 2),
            "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
            "-color_range", "tv", "-video_track_timescale", "15360", "-movflags", "+faststart"]


def final_pixels() -> str:
    return f"scale={WIDTH}:{HEIGHT}:out_color_matrix=bt709:out_range=tv,format=yuv420p,setsar=1"


def logo_bookend(ffmpeg: str, inputs: Path, work: Path, kind: str,
                 crf: int, preset: str) -> Path:
    output = work / f"{kind}.mp4"
    if kind == "outro":
        source = ["-i", str(inputs / "logo-ending.mp4")]
        seconds = OUTRO_SECONDS
        fades = "fade=t=in:st=0:d=0.18,fade=t=out:st=4.4:d=0.6,"
    elif kind == "intro":
        source = ["-loop", "1", "-framerate", str(FPS),
                  "-i", str(inputs / "logo-final.png")]
        seconds = INTRO_SECONDS
        fades = "fade=t=in:st=0:d=0.3,fade=t=out:st=1.75:d=0.25,"
    else:
        raise ValueError(f"Unknown logo bookend: {kind}")
    # Use the native logo at full frame for both ends of the film. The opening
    # holds the complete mark; the ending replays its editable letter animation.
    run([ffmpeg, "-hide_banner", "-loglevel", "warning", "-nostdin", "-y",
         *source, "-vf", fades + final_pixels(),
         *encode_options(crf, preset, seconds), str(output)])
    return output


def render_chapter(ffmpeg: str, inputs: Path, work: Path, fonts: dict[str, Path],
                   index: int, chapter: Chapter, elapsed: int, crf: int, preset: str) -> Path:
    output = work / f"chapter-{index:02d}.mp4"
    # The complete app is scaled, never cropped. Its bottom HUD remains visible.
    # The title zone ends at y=96; the app occupies y=104..1067.
    progress = round(APP_W * (elapsed + chapter.seconds) / 90)
    surround = [
        f"color=c={BACKGROUND}:s={WIDTH}x{HEIGHT}:r={FPS}:d={chapter.seconds}",
        "format=rgba",
        "drawbox=x=99:y=102:w=1722:h=972:color=black@0.30:t=fill",
        "drawbox=x=103:y=103:w=1714:h=965:color=white@0.12:t=fill",
    ]
    heading = [
        text_filter(work, f"{index:02d}-title", chapter.title, fonts["medium"], 32, PRIMARY, APP_X, 17),
        text_filter(work, f"{index:02d}-subtitle", chapter.subtitle, fonts["regular"], 18, SECONDARY, APP_X, 63),
        text_filter(work, f"{index:02d}-count", f"{index + 1:02d} / {len(CHAPTERS):02d}",
                    fonts["regular"], 16, SECONDARY, f"{APP_X + APP_W}-text_w", 31),
        f"drawbox=x={APP_X}:y=93:w={APP_W}:h=2:color=0x30333b:t=fill",
        f"drawbox=x={APP_X}:y=93:w={progress}:h=2:color={CORAL}@0.72:t=fill",
        final_pixels(),
    ]
    graph = (
        f"[0:v]fps={FPS},tpad=stop_mode=clone:stop_duration=0.2,"
        f"trim=duration={chapter.seconds},setpts=PTS-STARTPTS,"
        f"scale={APP_W}:{APP_H}:flags=lanczos,setsar=1,format=rgba[app];"
        + ",".join(surround) + "[surround];"
        + f"[surround][app]overlay=x={APP_X}:y={APP_Y}:shortest=1:format=rgb,"
        + ",".join(heading) + "[video]"
    )
    run([ffmpeg, "-hide_banner", "-loglevel", "warning", "-nostdin", "-y",
         "-i", str(inputs / (chapter.filename + ".mp4")), "-filter_complex", graph,
         "-map", "[video]", *encode_options(crf, preset, chapter.seconds), str(output)])
    return output


def timeline() -> list[dict]:
    entries = [{"start": 0, "end": INTRO_SECONDS, "file": "logo-final.png",
                "title": "omadesign — the finished mark", "kind": "intro"}]
    elapsed = INTRO_SECONDS
    for chapter in CHAPTERS:
        entries.append({"start": elapsed, "end": elapsed + chapter.seconds,
                        "file": chapter.filename + ".mp4", "title": chapter.title,
                        "subtitle": chapter.subtitle, "kind": "app"})
        elapsed += chapter.seconds
    entries.append({"start": elapsed, "end": elapsed + OUTRO_SECONDS,
                    "file": "logo-ending.mp4",
                    "title": "omadesign — the finished mark", "kind": "outro"})
    return entries


def metadata_escape(value: str) -> str:
    for character in ("\\", "=", ";", "#", "\n"):
        value = value.replace(character, "\\" + character)
    return value


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-dir", type=Path, default=Path.home() / "Videos/omadesign/logo-4-recut-sources")
    parser.add_argument("--music", type=Path, default=Path.home() / "Videos/omadesign/logo-4-recut-sources/music.wav")
    parser.add_argument("--output", type=Path, default=Path.home() / "Videos/omadesign/omadesign-logo-4-corrected-2026-09-05.mp4")
    parser.add_argument("--poster-time", type=float, default=95.5,
                        help="Poster frame in seconds; default is the finished logo")
    parser.add_argument("--crf", type=int, default=17)
    parser.add_argument("--preset", choices=("fast", "medium", "slow"), default="medium")
    parser.add_argument("--plan", action="store_true", help="Print timing/copy; do not encode or read media")
    args = parser.parse_args()
    if args.plan:
        print(json.dumps({"size": [WIDTH, HEIGHT], "fps": FPS, "seconds": TOTAL_SECONDS,
                          "app_rect": [APP_X, APP_Y, APP_W, APP_H],
                          "music": str(args.music), "output": str(args.output),
                          "timeline": timeline()}, indent=2, ensure_ascii=False))
        return
    if not 0 <= args.crf <= 35:
        parser.error("--crf must be between 0 and 35")
    if not math.isfinite(args.poster_time) or not 0 <= args.poster_time < TOTAL_SECONDS:
        parser.error("--poster-time must be a finite time between 0 and 97 seconds")
    ffmpeg, ffprobe = shutil.which("ffmpeg"), shutil.which("ffprobe")
    if not ffmpeg or not ffprobe:
        raise RuntimeError("Install ffmpeg and ffprobe before rendering")
    fonts = {weight: find_font(weight) for weight in ("regular", "medium")}
    # Validate the reviewed input contract before spending time on any encoding.
    for chapter in CHAPTERS:
        path = args.input_dir / (chapter.filename + ".mp4")
        if not path.is_file():
            raise FileNotFoundError(f"Missing capture: {path}")
        info = probe(path, ffprobe)
        video = next((stream for stream in info["streams"] if stream["codec_type"] == "video"), None)
        if video is None or (video.get("width"), video.get("height")) != (1600, 900):
            raise ValueError(f"Capture must contain the complete 1600×900 app: {path}")
        if duration(info, video) + 0.1 < chapter.seconds:
            raise ValueError(f"Capture is shorter than its {chapter.seconds}s chapter: {path}")
    if not args.music.is_file():
        raise FileNotFoundError(f"Missing music: {args.music}")
    audio_info = probe(args.music, ffprobe)
    audio = next((stream for stream in audio_info["streams"] if stream["codec_type"] == "audio"), None)
    if audio is None or duration(audio_info, audio) + 0.1 < TOTAL_SECONDS:
        raise ValueError("The background music must cover all 97 seconds")

    opening_info = probe(args.input_dir / "logo-final.png", ffprobe)
    opening_image = next((s for s in opening_info["streams"] if s["codec_type"] == "video"), None)
    if opening_image is None or (opening_image.get("width"), opening_image.get("height")) != (WIDTH, HEIGHT):
        raise ValueError("The opening logo must be the complete 1920×1080 native PNG render")
    ending_info = probe(args.input_dir / "logo-ending.mp4", ffprobe)
    ending_video = next(s for s in ending_info["streams"] if s["codec_type"] == "video")
    if (ending_video["width"], ending_video["height"]) != (WIDTH, HEIGHT) or duration(ending_info, ending_video) < OUTRO_SECONDS:
        raise ValueError("The final logo must be a five-second full-HD native render")
    work = args.input_dir / ".finish-render"
    work.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    (work / "timeline.json").write_text(json.dumps(timeline(), indent=2, ensure_ascii=False), encoding="utf-8")
    print("Rendering the opening logo…", flush=True)
    segments = [logo_bookend(ffmpeg, args.input_dir, work, "intro", args.crf, args.preset)]
    elapsed = 0
    for index, chapter in enumerate(CHAPTERS):
        print(f"Rendering {index + 1:02d}/{len(CHAPTERS)} — {chapter.title}", flush=True)
        segments.append(render_chapter(ffmpeg, args.input_dir, work, fonts,
                                       index, chapter, elapsed, args.crf, args.preset))
        elapsed += chapter.seconds
    print("Rendering the animated logo ending…", flush=True)
    segments.append(logo_bookend(ffmpeg, args.input_dir, work, "outro", args.crf, args.preset))

    concat = work / "segments.ffconcat"
    # These generated filenames are simple; quote arbitrary parent directories.
    concat.write_text("ffconcat version 1.0\n" + "".join(
        "file '" + str(path.resolve()).replace("'", "'\\''") + "'\n" for path in segments
    ), encoding="utf-8")
    metadata = work / "chapters.ffmetadata"
    metadata.write_text(";FFMETADATA1\ntitle=omadesign — A mark of our own\nartist=omadesign\n"
                        + "".join(f"[CHAPTER]\nTIMEBASE=1/1000\nSTART={entry['start'] * 1000}\nEND={entry['end'] * 1000}\ntitle={metadata_escape(entry['title'])}\n"
                                  for entry in timeline()), encoding="utf-8")
    partial = args.output.with_name(args.output.stem + ".partial.mp4")
    print("Joining chapters and balancing the continuous music bed…", flush=True)
    run([ffmpeg, "-hide_banner", "-loglevel", "warning", "-nostdin", "-y",
         "-f", "concat", "-safe", "0", "-i", str(concat), "-i", str(args.music),
         "-f", "ffmetadata", "-i", str(metadata), "-map", "0:v:0", "-map", "1:a:0",
         "-map_metadata", "2", "-map_chapters", "2", "-c:v", "copy", "-c:a", "aac",
         "-b:a", "192k", "-ar", "48000", "-ac", "2",
         "-af", "apad=pad_dur=0.2,atrim=duration=97,asetpts=PTS-STARTPTS,loudnorm=I=-21:TP=-2:LRA=7,afade=t=in:st=0:d=1,afade=t=out:st=94.5:d=2.5",
         "-t", str(TOTAL_SECONDS), "-movflags", "+faststart", str(partial)])
    info = probe(partial, ffprobe)
    video = next(stream for stream in info["streams"] if stream["codec_type"] == "video")
    if ((video["width"], video["height"]) != (WIDTH, HEIGHT)
            or abs(duration(info, video) - TOTAL_SECONDS) > 1 / FPS
            or int(video.get("nb_frames", 0)) != TOTAL_SECONDS * FPS
            or video.get("r_frame_rate") != f"{FPS}/1"):
        raise RuntimeError("Rendered movie failed the expected 1920×1080 / 2910-frame validation")
    os.replace(partial, args.output)
    poster = args.output.with_name(args.output.stem + "-poster.png")
    run([ffmpeg, "-hide_banner", "-loglevel", "warning", "-nostdin", "-y",
         "-ss", str(args.poster_time), "-i", str(args.output), "-frames:v", "1",
         "-vf", "format=rgb24", "-update", "1", str(poster)])
    print(f"Movie:  {args.output}\nPoster: {poster}\nLength: {TOTAL_SECONDS}s at {FPS} fps", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Assembly stopped: {error}", file=sys.stderr)
        sys.exit(1)
