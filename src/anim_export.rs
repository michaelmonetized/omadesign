//! Raster animation export: GIF, APNG, and ffmpeg MP4/WebM.
//!
//! Frames come from the same compositor as a still PNG, sampled on the motion clock.

use crate::document::Document;
use image::ImageEncoder;
use std::path::PathBuf;
use std::sync::OnceLock;

/// One raster animation format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimKind {
    Gif,
    Apng,
    Mp4,
    Webm,
}

impl AnimKind {
    pub const ALL: [Self; 4] = [Self::Gif, Self::Apng, Self::Mp4, Self::Webm];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            Self::Apng => "APNG",
            Self::Mp4 => "MP4",
            Self::Webm => "WebM",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::Apng => "png",
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
        }
    }

    /// MP4 has no alpha. The other three keep it.
    pub fn allows_alpha(self) -> bool {
        !matches!(self, Self::Mp4)
    }
}

/// Size, rate, range, loop, and background for one animation export.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimSettings {
    pub scale: f32,
    pub fps: f32,
    pub start: f32,
    pub end: f32,
    pub looped: bool,
    pub transparent: bool,
}

/// Open export window: the chosen format plus its settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimExport {
    pub kind: AnimKind,
    pub settings: AnimSettings,
}

impl AnimSettings {
    /// Defaults from the open document and the still-export scale.
    pub fn from_document(doc: &Document, scale: f32, kind: AnimKind) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let fps = if doc.motion.fps.is_finite() && doc.motion.fps > 0.0 {
            doc.motion.fps
        } else {
            30.0
        };
        let end = if doc.motion.duration.is_finite() {
            doc.motion.duration.max(0.0)
        } else {
            0.0
        };
        Self {
            scale,
            fps,
            start: 0.0,
            end,
            looped: doc.motion.looped,
            transparent: kind.allows_alpha(),
        }
    }
}

/// Straight RGBA pixels. GIF and APNG tests feed these in without the renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Sample times for an animation.
///
/// The count is `round((end - start) * fps)`, and at least one.
/// Time `i` is `start + i / fps`. The last sample stays within half a frame of `end`.
pub fn frame_times(settings: &AnimSettings) -> Vec<f32> {
    let fps = if settings.fps.is_finite() && settings.fps > 0.0 {
        settings.fps as f64
    } else {
        1.0
    };
    let start = if settings.start.is_finite() {
        settings.start as f64
    } else {
        0.0
    };
    let end = if settings.end.is_finite() {
        settings.end as f64
    } else {
        start
    };
    let span = (end - start).max(0.0);
    let rounded = (span * fps).round();
    let count = if rounded.is_finite() && rounded > 0.0 {
        (rounded as u64).min(100_000) as usize
    } else {
        1
    };
    let limit = end + 0.5 / fps;
    let mut times = Vec::with_capacity(count);
    for i in 0..count {
        let t = start + (i as f64) / fps;
        if t > limit + 1e-6 {
            break;
        }
        times.push(t as f32);
    }
    if times.is_empty() {
        times.push(start as f32);
    }
    times
}

/// Status line for the open settings, such as `GIF · 10 fps · 2.0s · 20 frames`.
pub fn summary(kind: AnimKind, settings: &AnimSettings) -> String {
    let frames = frame_times(settings).len();
    let span = (settings.end - settings.start).max(0.0);
    format!(
        "{} · {} fps · {:.1}s · {} frames",
        kind.label(),
        format_rate(settings.fps),
        span,
        frames
    )
}

/// Render one frame to PNG bytes through the motion-aware compositor.
pub fn render_frame(
    doc: &Document,
    time: f32,
    scale: f32,
    transparent: bool,
) -> Result<Vec<u8>, String> {
    crate::compositor::render_export_at(doc, scale, time, transparent)?
        .encode_png()
        .map_err(|e| e.to_string())
}

/// Encode straight RGBA frames as an animated GIF. Loops when `looped` is set.
pub fn encode_gif(frames: &[RgbaFrame], fps: f32, looped: bool) -> Result<Vec<u8>, String> {
    write_gif(frames, fps, looped, || true)
}

/// Encode straight RGBA frames as an APNG. `num_plays` is zero when `looped` is set.
pub fn encode_apng(frames: &[RgbaFrame], fps: f32, looped: bool) -> Result<Vec<u8>, String> {
    if frames.is_empty() {
        return Err("no frames".into());
    }
    let width = frames[0].width;
    let height = frames[0].height;
    if width == 0 || height == 0 {
        return Err("frame has no pixels".into());
    }
    let mut pngs = Vec::with_capacity(frames.len());
    for frame in frames {
        if frame.width != width || frame.height != height {
            return Err("frames must share one size".into());
        }
        pngs.push(encode_png(frame)?);
    }
    assemble_apng(&pngs, fps, looped)
}

/// ffmpeg arguments for an image sequence.
///
/// MP4 is H.264 `yuv420p`. WebM is VP9, with alpha when the settings ask for it.
/// `-frames:v` is the planned frame count, so the file lasts `frames / fps`.
pub fn ffmpeg_args(
    kind: AnimKind,
    settings: &AnimSettings,
    pattern: &str,
    output: &str,
) -> Vec<String> {
    let rate = format_rate(settings.fps);
    let frames = frame_times(settings).len().to_string();
    let mut args = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-framerate".into(),
        rate.clone(),
        "-start_number".into(),
        "0".into(),
        "-i".into(),
        pattern.into(),
        "-frames:v".into(),
        frames,
        "-r".into(),
        rate,
        "-an".into(),
    ];
    match kind {
        AnimKind::Mp4 => args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]),
        AnimKind::Webm => {
            let pix = if settings.transparent {
                "yuva420p"
            } else {
                "yuv420p"
            };
            args.extend([
                "-c:v".into(),
                "libvpx-vp9".into(),
                "-pix_fmt".into(),
                pix.into(),
                "-auto-alt-ref".into(),
                "0".into(),
                "-b:v".into(),
                "0".into(),
                "-crf".into(),
                "32".into(),
                "-deadline".into(),
                "good".into(),
                "-cpu-used".into(),
                "4".into(),
                "-row-mt".into(),
                "1".into(),
            ]);
        }
        AnimKind::Gif | AnimKind::Apng => {}
    }
    args.push(output.into());
    args
}

/// Path to `ffmpeg`, or the sentence `ffmpeg is not installed`.
pub fn ffmpeg_bin() -> Result<String, String> {
    let output = std::process::Command::new("which")
        .arg("ffmpeg")
        .output()
        .map_err(|_| "ffmpeg is not installed".to_string())?;
    if !output.status.success() {
        return Err("ffmpeg is not installed".into());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        Err("ffmpeg is not installed".into())
    } else {
        Ok(path)
    }
}

/// Cached `which ffmpeg` for the export window.
pub fn ffmpeg_installed() -> bool {
    static FOUND: OnceLock<bool> = OnceLock::new();
    *FOUND.get_or_init(|| ffmpeg_bin().is_ok())
}

/// Render the plan and encode it.
///
/// `report(done, total)` runs before each frame and again before the container encode.
/// Return `false` to stop. A missing ffmpeg fails only MP4 and WebM.
pub fn export_animation(
    doc: &Document,
    kind: AnimKind,
    settings: &AnimSettings,
    mut report: impl FnMut(u32, u32) -> bool,
) -> Result<Vec<u8>, String> {
    let times = frame_times(settings);
    let total = times.len() as u32;
    let transparent = settings.transparent && kind.allows_alpha();
    match kind {
        AnimKind::Mp4 | AnimKind::Webm => {
            encode_video(doc, kind, settings, &times, transparent, &mut report)
        }
        AnimKind::Gif | AnimKind::Apng => {
            let mut frames = Vec::with_capacity(times.len());
            for (index, time) in times.iter().enumerate() {
                if !report(index as u32, total) {
                    return Err("Export cancelled".into());
                }
                let png = render_frame(doc, *time, settings.scale, transparent)?;
                frames.push(rgba_from_png(&png)?);
            }
            if !report(total, total) {
                return Err("Export cancelled".into());
            }
            let mut keep = || report(total, total);
            match kind {
                AnimKind::Gif => write_gif(&frames, settings.fps, settings.looped, &mut keep),
                AnimKind::Apng => encode_apng(&frames, settings.fps, settings.looped),
                AnimKind::Mp4 | AnimKind::Webm => unreachable!(),
            }
        }
    }
}

fn write_gif(
    frames: &[RgbaFrame],
    fps: f32,
    looped: bool,
    mut keep: impl FnMut() -> bool,
) -> Result<Vec<u8>, String> {
    if frames.is_empty() {
        return Err("no frames".into());
    }
    let width = frames[0].width;
    let height = frames[0].height;
    if width == 0 || height == 0 || width > u16::MAX as u32 || height > u16::MAX as u32 {
        return Err("frame is too large for GIF".into());
    }
    let delay = image::Delay::from_numer_denom_ms(frame_delay_ms(fps), 1);
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut bytes, 10);
        if looped {
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .map_err(|e| e.to_string())?;
        }
        for frame in frames {
            if !keep() {
                return Err("Export cancelled".into());
            }
            if frame.width != width || frame.height != height {
                return Err("frames must share one size".into());
            }
            let image = image::RgbaImage::from_raw(width, height, frame.rgba.clone())
                .ok_or("frame has the wrong number of pixels")?;
            encoder
                .encode_frame(image::Frame::from_parts(image, 0, 0, delay))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(bytes)
}

fn encode_video(
    doc: &Document,
    kind: AnimKind,
    settings: &AnimSettings,
    times: &[f32],
    transparent: bool,
    report: &mut dyn FnMut(u32, u32) -> bool,
) -> Result<Vec<u8>, String> {
    let dir = scratch_dir()?;
    let guard = ScratchDir(dir.clone());
    let total = times.len() as u32;
    for (index, time) in times.iter().enumerate() {
        if !report(index as u32, total) {
            return Err("Export cancelled".into());
        }
        let png = render_frame(doc, *time, settings.scale, transparent)?;
        let png = even_png(&png, transparent)?;
        std::fs::write(dir.join(format!("frame_{index:04}.png")), png)
            .map_err(|e| e.to_string())?;
    }
    if !report(total, total) {
        return Err("Export cancelled".into());
    }
    let pattern = dir.join("frame_%04d.png");
    let output = dir.join(format!("out.{}", kind.extension()));
    let args = ffmpeg_args(
        kind,
        settings,
        &pattern.to_string_lossy(),
        &output.to_string_lossy(),
    );
    run_ffmpeg(&args, &mut || report(total, total))?;
    let bytes = std::fs::read(&output).map_err(|e| e.to_string())?;
    drop(guard);
    Ok(bytes)
}

fn run_ffmpeg(args: &[String], cancel: &mut dyn FnMut() -> bool) -> Result<(), String> {
    let bin = ffmpeg_bin()?;
    if !cancel() {
        return Err("Export cancelled".into());
    }
    let mut child = std::process::Command::new(bin)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "ffmpeg is not installed".to_string()
            } else {
                error.to_string()
            }
        })?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stderr.as_mut() {
            use std::io::Read;
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let status = loop {
        if !cancel() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("Export cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(40)),
            Err(error) => return Err(error.to_string()),
        }
    };
    let err_bytes = reader.join().unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&err_bytes);
    let line = err
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("ffmpeg failed");
    Err(format!("ffmpeg failed: {line}"))
}

fn even_png(png: &[u8], transparent: bool) -> Result<Vec<u8>, String> {
    let frame = rgba_from_png(png)?;
    if frame.width % 2 == 0 && frame.height % 2 == 0 {
        return Ok(png.to_vec());
    }
    encode_png(&pad_even(&frame, transparent))
}

fn pad_even(frame: &RgbaFrame, transparent: bool) -> RgbaFrame {
    let width = frame.width + (frame.width & 1);
    let height = frame.height + (frame.height & 1);
    if width == frame.width && height == frame.height {
        return frame.clone();
    }
    let fill = if transparent {
        [0, 0, 0, 0]
    } else {
        [255, 255, 255, 255]
    };
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height {
        for x in 0..width {
            let dst = ((y * width + x) * 4) as usize;
            if x < frame.width && y < frame.height {
                let src = ((y * frame.width + x) * 4) as usize;
                rgba[dst..dst + 4].copy_from_slice(&frame.rgba[src..src + 4]);
            } else {
                rgba[dst..dst + 4].copy_from_slice(&fill);
            }
        }
    }
    RgbaFrame {
        width,
        height,
        rgba,
    }
}

fn rgba_from_png(bytes: &[u8]) -> Result<RgbaFrame, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    Ok(RgbaFrame {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

fn encode_png(frame: &RgbaFrame) -> Result<Vec<u8>, String> {
    let expected = frame.width as usize * frame.height as usize * 4;
    if frame.width == 0 || frame.height == 0 || frame.rgba.len() != expected {
        return Err("frame has the wrong number of pixels".into());
    }
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &frame.rgba,
            frame.width,
            frame.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

struct Chunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
    raw: &'a [u8],
}

fn parse_chunks(png: &[u8]) -> Result<Vec<Chunk<'_>>, String> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !png.starts_with(SIG) {
        return Err("frame is not a png".into());
    }
    let mut i = 8;
    let mut chunks = Vec::new();
    while i + 12 <= png.len() {
        let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
        let kind = png[i + 4..i + 8].try_into().unwrap();
        let data_at = i + 8;
        let end = data_at
            .checked_add(len)
            .and_then(|end| end.checked_add(4))
            .ok_or("bad png chunk")?;
        if end > png.len() {
            return Err("truncated png".into());
        }
        chunks.push(Chunk {
            kind,
            data: &png[data_at..data_at + len],
            raw: &png[i..end],
        });
        i = end;
        if kind == *b"IEND" {
            break;
        }
    }
    if chunks.is_empty() {
        return Err("png has no chunks".into());
    }
    Ok(chunks)
}

fn assemble_apng(pngs: &[Vec<u8>], fps: f32, looped: bool) -> Result<Vec<u8>, String> {
    let first = parse_chunks(&pngs[0])?;
    let ihdr = first
        .iter()
        .find(|chunk| chunk.kind == *b"IHDR")
        .ok_or("png is missing IHDR")?;
    if ihdr.data.len() < 13 {
        return Err("bad IHDR".into());
    }
    let width = u32::from_be_bytes(ihdr.data[0..4].try_into().unwrap());
    let height = u32::from_be_bytes(ihdr.data[4..8].try_into().unwrap());
    let (delay_num, delay_den) = delay_parts(fps);
    let mut out = Vec::new();
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    out.extend_from_slice(ihdr.raw);
    let mut actl = [0u8; 8];
    actl[..4].copy_from_slice(&(pngs.len() as u32).to_be_bytes());
    let plays: u32 = if looped { 0 } else { 1 };
    actl[4..].copy_from_slice(&plays.to_be_bytes());
    push_chunk(&mut out, b"acTL", &actl);
    for chunk in &first {
        if chunk.kind == *b"IDAT" {
            break;
        }
        if chunk.kind == *b"IHDR" || chunk.kind == *b"acTL" || chunk.kind == *b"fcTL" {
            continue;
        }
        out.extend_from_slice(chunk.raw);
    }
    let mut seq = 0u32;
    for (index, png) in pngs.iter().enumerate() {
        let chunks = parse_chunks(png)?;
        let mut fctl = [0u8; 26];
        fctl[..4].copy_from_slice(&seq.to_be_bytes());
        fctl[4..8].copy_from_slice(&width.to_be_bytes());
        fctl[8..12].copy_from_slice(&height.to_be_bytes());
        fctl[20..22].copy_from_slice(&delay_num.to_be_bytes());
        fctl[22..24].copy_from_slice(&delay_den.to_be_bytes());
        fctl[24] = 1;
        fctl[25] = 0;
        push_chunk(&mut out, b"fcTL", &fctl);
        seq = seq.saturating_add(1);
        if index == 0 {
            let mut saw = false;
            for chunk in &chunks {
                if chunk.kind == *b"IDAT" {
                    out.extend_from_slice(chunk.raw);
                    saw = true;
                }
            }
            if !saw {
                return Err("png is missing IDAT".into());
            }
        } else {
            let mut zlib = Vec::new();
            for chunk in &chunks {
                if chunk.kind == *b"IDAT" {
                    zlib.extend_from_slice(chunk.data);
                }
            }
            if zlib.is_empty() {
                return Err("png is missing IDAT".into());
            }
            write_fdats(&mut out, &mut seq, &zlib);
        }
    }
    push_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

fn write_fdats(out: &mut Vec<u8>, seq: &mut u32, zlib: &[u8]) {
    const CHUNK: usize = 1024 * 1024;
    for piece in zlib.chunks(CHUNK) {
        let mut payload = Vec::with_capacity(4 + piece.len());
        payload.extend_from_slice(&seq.to_be_bytes());
        payload.extend_from_slice(piece);
        push_chunk(out, b"fdAT", &payload);
        *seq = seq.saturating_add(1);
    }
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let Ok(len) = u32::try_from(data.len()) else {
        return;
    };
    out.extend_from_slice(&len.to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let sum = crc32(&out[start..]);
    out.extend_from_slice(&sum.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        let index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
        crc = CRC_TABLE[index] ^ (crc >> 8);
    }
    !crc
}

static CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            if c & 1 == 1 {
                c = 0xEDB8_8320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            bit += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

fn format_rate(fps: f32) -> String {
    if !fps.is_finite() || fps <= 0.0 {
        return "1".into();
    }
    if (fps - fps.round()).abs() < 0.001 {
        format!("{}", fps.round() as i32)
    } else {
        let text = format!("{fps:.3}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn frame_delay_ms(fps: f32) -> u32 {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        1.0
    };
    (1000.0 / fps).round().clamp(10.0, 60_000.0) as u32
}

fn delay_parts(fps: f32) -> (u16, u16) {
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        1.0
    };
    let rounded = fps.round();
    if (fps - rounded).abs() < 0.01 && (1.0..=65535.0).contains(&rounded) {
        (1, rounded as u16)
    } else {
        let num = (1000.0 / fps).round().clamp(1.0, 65535.0) as u16;
        (num, 1000)
    }
}

struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!(
        "omadesign-anim-{}-{}",
        std::process::id(),
        crate::document::next_id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::document::{Cmd, Document, Fill, Shape, Style, apply};
    use crate::geom::{Geom, Pt};
    use crate::gradient::{Gradient, GradientKind};
    use crate::motion::{Ease, Prop};
    use image::AnimationDecoder;
    use std::io::Cursor;

    fn settings(start: f32, end: f32, fps: f32) -> AnimSettings {
        AnimSettings {
            scale: 1.0,
            fps,
            start,
            end,
            looped: true,
            transparent: true,
        }
    }

    #[test]
    fn frame_times_cover_the_span_without_running_past_the_end() {
        let full = settings(0.0, 2.0, 10.0);
        let times = frame_times(&full);
        assert_eq!(times.len(), 20);
        assert!((times[0] - 0.0).abs() < 1e-4);
        assert!((times[19] - 1.9).abs() < 1e-3);
        assert!(*times.last().unwrap() <= 2.0 + 0.5 / 10.0);
        assert_eq!(
            summary(AnimKind::Gif, &full),
            "GIF · 10 fps · 2.0s · 20 frames"
        );

        let slice = settings(1.0, 2.0, 10.0);
        let times = frame_times(&slice);
        assert_eq!(times.len(), 10);
        assert!((times[0] - 1.0).abs() < 1e-4);
        assert!(*times.last().unwrap() <= 2.0 + 0.5 / 10.0);
    }

    #[test]
    fn png_crc_matches_the_iso_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    fn solid(color: [u8; 4], n: usize) -> RgbaFrame {
        RgbaFrame {
            width: 2,
            height: 2,
            rgba: color.repeat(n).into_iter().take(16).collect(),
        }
    }

    #[test]
    fn gif_bytes_hold_two_frames_and_loop() {
        let frames = [solid([255, 0, 0, 255], 4), solid([0, 0, 255, 255], 4)];
        let bytes = encode_gif(&frames, 10.0, true).unwrap();
        assert!(bytes.starts_with(b"GIF89a"));
        assert!(bytes.windows(11).any(|w| w == b"NETSCAPE2.0"));
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).unwrap();
        let decoded = decoder
            .into_frames()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(decoded.len() > 1);
    }

    #[test]
    fn apng_actl_matches_the_plan_and_keeps_alpha() {
        let plan = settings(0.0, 0.2, 10.0);
        let count = frame_times(&plan).len();
        assert_eq!(count, 2);
        let mut clear = solid([0, 0, 0, 0], 4);
        clear.rgba[4..8].copy_from_slice(&[255, 0, 0, 255]);
        let mut blue = solid([0, 0, 255, 128], 4);
        blue.rgba[3] = 0;
        let bytes = encode_apng(&[clear, blue], plan.fps, plan.looped).unwrap();
        assert!(bytes.windows(4).any(|w| w == b"acTL"));
        let (frames, plays) = actl(&bytes);
        assert_eq!(frames, count as u32);
        assert_eq!(plays, 0);
        let once = encode_apng(&[solid([255, 0, 0, 255], 4)], 10.0, false).unwrap();
        assert_eq!(actl(&once).1, 1);

        let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
            .unwrap()
            .apng()
            .unwrap();
        let decoded = decoder
            .into_frames()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded.len(), count);
        let first = decoded[0].buffer();
        assert_eq!(first.get_pixel(0, 0)[3], 0);
        assert_eq!(first.get_pixel(1, 0)[3], 255);
    }

    fn actl(bytes: &[u8]) -> (u32, u32) {
        let pos = bytes.windows(4).position(|w| w == b"acTL").expect("acTL");
        let data = &bytes[pos + 4..pos + 12];
        (
            u32::from_be_bytes(data[0..4].try_into().unwrap()),
            u32::from_be_bytes(data[4..8].try_into().unwrap()),
        )
    }

    #[test]
    fn ffmpeg_args_name_the_codec_and_the_frame_rate() {
        let plan = settings(0.0, 1.0, 12.0);
        let mp4 = ffmpeg_args(AnimKind::Mp4, &plan, "frame_%04d.png", "out.mp4");
        assert!(mp4.iter().any(|arg| arg == "libx264"));
        assert!(mp4.iter().any(|arg| arg == "yuv420p"));
        assert!(mp4.iter().any(|arg| arg == "-an"));
        assert!(
            mp4.windows(2)
                .any(|pair| pair[0] == "-r" && pair[1] == "12")
        );
        assert!(
            mp4.windows(2)
                .any(|pair| pair[0] == "-frames:v" && pair[1] == "12")
        );
        let mut web = plan;
        web.transparent = true;
        let webm = ffmpeg_args(AnimKind::Webm, &web, "frame_%04d.png", "out.webm");
        assert!(webm.iter().any(|arg| arg == "libvpx-vp9"));
        assert!(webm.iter().any(|arg| arg == "yuva420p"));
        assert!(
            webm.windows(2)
                .any(|pair| pair[0] == "-r" && pair[1] == "12")
        );
    }

    #[test]
    fn render_frame_turns_a_keyed_gradient() {
        let mut doc = Document::new("spin", 32.0, 32.0, 72.0);
        doc.transparent = true;
        doc.layers[0].visible = false;
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(2.0, 2.0),
                size: Pt::new(28.0, 28.0),
                radius: 0.0,
            },
            Style {
                fill: Fill::Gradient(Gradient::new(
                    GradientKind::Linear,
                    Rgba::rgb(255, 0, 0),
                    Rgba::rgb(0, 0, 255),
                )),
                stroke: None,
            },
        );
        let id = shape.id;
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        doc.motion.duration = 1.0;
        doc.motion
            .set_key(id, Prop::GradientAngle, 0.0, 0.0, Ease::Linear);
        doc.motion
            .set_key(id, Prop::GradientAngle, 1.0, 180.0, Ease::Linear);
        let start = render_frame(&doc, 0.0, 1.0, true).unwrap();
        let mid = render_frame(&doc, 0.5, 1.0, true).unwrap();
        assert!(start.starts_with(b"\x89PNG"));
        assert_ne!(start, mid);
    }

    #[test]
    fn video_export_names_a_missing_ffmpeg() {
        if ffmpeg_bin().is_ok() {
            return;
        }
        let doc = Document::new("clip", 8.0, 8.0, 72.0);
        let err = export_animation(&doc, AnimKind::Mp4, &settings(0.0, 0.5, 2.0), |_, _| true)
            .unwrap_err();
        assert_eq!(err, "ffmpeg is not installed");
    }

    #[test]
    fn ffmpeg_writes_a_tiny_mp4_when_the_binary_exists() {
        if !ffmpeg_has("libx264") {
            return;
        }
        let mut doc = Document::new("clip", 16.0, 16.0, 72.0);
        doc.transparent = true;
        doc.layers[0].visible = false;
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(2.0, 2.0),
                size: Pt::new(12.0, 12.0),
                radius: 0.0,
            },
            Style {
                fill: Fill::Solid(Rgba::rgb(220, 20, 40)),
                stroke: None,
            },
        );
        let id = shape.id;
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        doc.motion.duration = 1.0;
        doc.motion.set_key(id, Prop::X, 0.0, 0.0, Ease::Linear);
        doc.motion.set_key(id, Prop::X, 1.0, 4.0, Ease::Linear);
        let mut plan = settings(0.0, 1.0, 2.0);
        plan.transparent = false;
        let bytes = export_animation(&doc, AnimKind::Mp4, &plan, |_, _| true).unwrap();
        assert!(bytes.len() > 32);
        assert!(bytes.windows(4).any(|w| w == b"ftyp"));
    }

    fn ffmpeg_has(encoder: &str) -> bool {
        let Ok(bin) = ffmpeg_bin() else {
            return false;
        };
        let Ok(out) = std::process::Command::new(bin).arg("-encoders").output() else {
            return false;
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|line| line.split_whitespace().any(|word| word == encoder))
    }
}
