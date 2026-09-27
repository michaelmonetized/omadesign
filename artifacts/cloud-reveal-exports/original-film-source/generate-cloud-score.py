#!/usr/bin/env python3
"""Render Omadesign's original 20-second cloud reveal score, without samples.

Requires Python + numpy and ffmpeg. Run from any directory. The lossless master
is temporary unless --master is supplied; only the compact AAC ships on the site.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import subprocess
import tempfile
import wave

import numpy as np


RATE = 48_000
DURATION = 20.0
FRAMES = int(RATE * DURATION)
RNG = np.random.default_rng(23092026)
MIX = np.zeros((FRAMES, 2), dtype=np.float64)


def frequency(midi: float) -> float:
    return 440.0 * 2.0 ** ((midi - 69.0) / 12.0)


def add(signal: np.ndarray, start: float, amplitude: float, pan: float) -> None:
    first = round(start * RATE)
    count = min(signal.size, FRAMES - first)
    if count <= 0:
        return
    # Equal-power stereo placement, with no hard-panned lead voices.
    angle = (pan + 1.0) * math.pi / 4.0
    MIX[first : first + count, 0] += signal[:count] * amplitude * math.cos(angle)
    MIX[first : first + count, 1] += signal[:count] * amplitude * math.sin(angle)


def swell(midi: float, start: float, duration: float, amplitude: float,
          pan: float, attack: float = 1.25, release: float = 1.65) -> None:
    """A quiet five-player bowed-string-like voice with evolving partials."""
    t = np.arange(round(duration * RATE)) / RATE
    envelope = np.sin(np.minimum(t / attack, 1.0) * math.pi / 2.0) ** 1.5
    envelope *= np.sin(np.minimum((duration - t) / release, 1.0) * math.pi / 2.0) ** 1.5
    result = np.zeros_like(t)
    fundamental = frequency(midi)
    for player in range(5):
        detune = (player - 2) * 0.00135 + RNG.uniform(-0.00025, 0.00025)
        initial_phase = RNG.uniform(0.0, math.tau)
        # Integrated vibrato avoids frequency/phase discontinuities.
        phase = math.tau * fundamental * (1.0 + detune) * t
        phase += 0.13 * np.sin(math.tau * (4.5 + player * 0.23) * t + initial_phase)
        phase += initial_phase
        for harmonic in range(1, 8):
            strength = harmonic ** -1.85 * math.exp(-(harmonic - 1) * 0.24)
            strength *= 1.0 + 0.08 * np.sin(math.tau * 0.17 * t + harmonic)
            result += np.sin(phase * harmonic) * strength / 5.0
    add(result * envelope, start, amplitude, pan)


def bell(midi: float, start: float, amplitude: float, pan: float,
         duration: float = 4.0) -> None:
    """Soft felt-struck celesta/chime, using original additive partials."""
    t = np.arange(round(duration * RATE)) / RATE
    tone = np.zeros_like(t)
    for ratio, level, decay in [(1.0, 1.0, 1.35), (2.0, 0.21, 0.85),
                                (3.003, 0.07, 0.48), (4.008, 0.025, 0.32)]:
        tone += level * np.sin(math.tau * frequency(midi) * ratio * t) * np.exp(-t / decay)
    tone *= 1.0 - np.exp(-t / 0.022)
    tone *= np.minimum((duration - t) / 0.15, 1.0)
    add(tone, start, amplitude, pan)


def room(signal: np.ndarray) -> np.ndarray:
    """Deterministic diffuse stereo hall tail generated from filtered noise."""
    length = round(RATE * 3.4)
    t = np.arange(length) / RATE
    result = signal.copy()
    fft_size = 1 << (FRAMES + length - 2).bit_length()
    for channel in range(2):
        impulse = RNG.standard_normal(length)
        # Absorption removes the metallic high-frequency tail of bare noise.
        impulse = np.convolve(impulse, np.ones(17) / 17.0, mode="same")
        impulse *= np.exp(-t / 0.60) * np.minimum(t / 0.040, 1.0)
        impulse[: round(0.044 * RATE)] = 0.0
        impulse /= np.sqrt(np.sum(impulse * impulse))
        wet = np.fft.irfft(np.fft.rfft(signal[:, channel], fft_size)
                           * np.fft.rfft(impulse, fft_size), fft_size)[:FRAMES]
        # Quiet crossfeed unifies the image without washing away the melody.
        result[:, channel] += wet * 0.21
        result[:, 1 - channel] += wet * 0.055
    return result


def compose() -> np.ndarray:
    # Original spacious D-major progression. Each chord overlaps the next;
    # upper voices rise during the trace and relax as the artwork recedes.
    chords = [
        (0.0, 6.0, [38, 50, 57, 61, 64], 0.080),       # Dmaj9, moonlit glide
        (3.7, 5.7, [43, 50, 57, 59, 66], 0.090),       # Gmaj9, O tracing
        (7.2, 5.6, [45, 52, 57, 62, 64], 0.104),       # Asus/add9, wordmark
        (10.7, 5.9, [38, 50, 57, 62, 66, 69], 0.098),  # D, hero resolution
        (14.0, 5.8, [38, 50, 57, 64, 66], 0.059),      # Dadd9, quiet recession
    ]
    for start, duration, notes, level in chords:
        for index, midi in enumerate(notes):
            pan = float(np.linspace(-0.52, 0.52, len(notes))[index])
            weight = 0.80 if index == 0 else 1.0 / math.sqrt(len(notes) - 1)
            swell(midi, start + index * 0.042, duration, level * weight, pan)

    # A restrained original upward phrase; no beat, drums, or aggressive lead.
    for midi, start, duration, level, pan in [
        (69, 3.7, 3.3, 0.024, -0.13),
        (71, 5.3, 3.4, 0.026, 0.13),
        (74, 7.0, 3.3, 0.030, -0.10),
        (76, 8.8, 3.0, 0.028, 0.10),
        (78, 10.7, 4.2, 0.025, 0.0),
    ]:
        swell(midi, start, duration, level, pan, attack=0.8, release=1.3)

    for midi, start, amplitude, pan in [
        (81, 2.25, 0.016, -0.35),
        (86, 4.6, 0.024, 0.36),
        (81, 6.9, 0.018, -0.23),
        (88, 8.4, 0.020, 0.25),
        (86, 10.9, 0.028, -0.10),
        (78, 11.05, 0.016, 0.15),
        (81, 14.5, 0.012, 0.25),
        (86, 16.35, 0.019, -0.12),
    ]:
        bell(midi, start, amplitude, pan)

    result = room(MIX)
    # Gentle boundaries and a clean compositing tail; no limiter pumping.
    timeline = np.arange(FRAMES) / RATE
    result *= np.minimum(timeline / 0.16, 1.0)[:, None]
    result *= (np.clip((DURATION - timeline) / 2.15, 0.0, 1.0) ** 1.5)[:, None]
    result -= result.mean(axis=0)
    result *= 10.0 ** (-4.8 / 20.0) / np.max(np.abs(result))
    assert np.isfinite(result).all()
    return result


def write_wav(path: Path, signal: np.ndarray) -> None:
    with wave.open(str(path), "wb") as output:
        output.setnchannels(2)
        output.setsampwidth(2)
        output.setframerate(RATE)
        output.writeframes((np.clip(signal, -1.0, 1.0) * 32767).astype("<i2").tobytes())


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path,
                        default=root / "site/public/media/cloud/reveal-score.m4a")
    parser.add_argument("--master", type=Path, help="Optional lossless WAV master destination")
    args = parser.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    signal = compose()
    with tempfile.TemporaryDirectory(prefix="omadesign-score-") as temp:
        master = args.master or Path(temp) / "reveal-score.wav"
        master.parent.mkdir(parents=True, exist_ok=True)
        write_wav(master, signal)
        subprocess.run([
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(master),
            "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart",
            "-metadata", "title=Omadesign — Moonlit Reveal",
            "-metadata", "comment=Original procedural instrumental; no sampled recordings",
            str(args.output),
        ], check=True)
    print(json.dumps({
        "path": str(args.output), "duration_seconds": DURATION, "sample_rate": RATE,
        "channels": 2, "finite": bool(np.isfinite(signal).all()),
        "peak_dbfs": round(20 * math.log10(float(np.max(np.abs(signal)))), 2),
        "rms_dbfs": round(20 * math.log10(float(np.sqrt(np.mean(signal ** 2)))), 2),
        "size_bytes": args.output.stat().st_size,
    }, indent=2))


if __name__ == "__main__":
    main()
