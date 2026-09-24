//! One screen pixel, including windows outside this app.
use crate::color::Rgba;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn parse_cursor_pos(text: &str) -> Option<(i32, i32)> {
    let (x, y) = text.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub fn parse_hex_color(text: &str) -> Option<Rgba> {
    let token = text.split_whitespace().next()?;
    Rgba::parse_hex(token)
}

pub fn parse_ppm_pixel(data: &[u8]) -> Option<Rgba> {
    if data.first().copied() != Some(b'P') || data.get(1).copied() != Some(b'6') {
        return None;
    }
    let mut index = 2;
    let mut numbers = [0u32; 3];
    let mut count = 0;
    while count < 3 && index < data.len() {
        while index < data.len() && data[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < data.len() && data[index] == b'#' {
            while index < data.len() && data[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        let start = index;
        while index < data.len() && data[index].is_ascii_digit() {
            index += 1;
        }
        if start == index {
            return None;
        }
        numbers[count] = std::str::from_utf8(&data[start..index])
            .ok()?
            .parse()
            .ok()?;
        count += 1;
    }
    if count != 3 || numbers[0] != 1 || numbers[1] != 1 || numbers[2] > 255 {
        return None;
    }
    if index < data.len() && data[index].is_ascii_whitespace() {
        index += 1;
    }
    let pixel = data.get(index..index + 3)?;
    Some(Rgba::rgb(pixel[0], pixel[1], pixel[2]))
}

pub fn sample_at(x: i32, y: i32) -> Result<Rgba, String> {
    let output = Command::new("grim")
        .args(["-g", &format!("{x},{y} 1x1"), "-t", "ppm", "-"])
        .output()
        .map_err(|error| format!("Could not read the screen ({error})"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            "Could not read the screen".into()
        } else {
            format!("Could not read the screen ({detail})")
        });
    }
    parse_ppm_pixel(&output.stdout).ok_or_else(|| "The screen pixel was unreadable".into())
}

struct Live(Child);

static LIVE: Mutex<Option<Live>> = Mutex::new(None);
/// Stay down until the eyedropper is chosen, and after a pick, a cancel, or a failed launch.
static HOLD: AtomicBool = AtomicBool::new(true);

pub struct Tick {
    pub just_started: bool,
    pub live: bool,
    pub finished: Option<Result<Rgba, String>>,
}

/// The eyedropper was chosen. The next sync starts a screen grab.
pub fn rearm() {
    HOLD.store(false, Ordering::Relaxed);
}

/// True while the next click should land anywhere on the desktop.
pub fn grabbing_screen() -> bool {
    !HOLD.load(Ordering::Relaxed)
}

/// Drop the grab. A later eyedropper choice starts a new one.
pub fn stop() {
    HOLD.store(true, Ordering::Relaxed);
    kill();
}

/// Keep a screen grab alive while the eyedropper is the tool.
pub fn sync(active: bool) -> Tick {
    if !active {
        kill();
        return Tick {
            just_started: false,
            live: false,
            finished: None,
        };
    }
    if HOLD.load(Ordering::Relaxed) {
        return Tick {
            just_started: false,
            live: false,
            finished: None,
        };
    }
    let just_started = match ensure() {
        Ok(started) => started,
        Err(error) => {
            HOLD.store(true, Ordering::Relaxed);
            return Tick {
                just_started: false,
                live: false,
                finished: Some(Err(error)),
            };
        }
    };
    let finished = take();
    let live = lock().is_some();
    Tick {
        just_started,
        live,
        finished,
    }
}

fn lock() -> std::sync::MutexGuard<'static, Option<Live>> {
    LIVE.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn ensure() -> Result<bool, String> {
    let mut slot = lock();
    if slot.is_some() {
        return Ok(false);
    }
    match Command::new("hyprpicker")
        .args(["-q", "-f", "hex", "-l"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            *slot = Some(Live(child));
            Ok(true)
        }
        Err(error) => Err(format!("Could not start a screen picker ({error})")),
    }
}

fn take() -> Option<Result<Rgba, String>> {
    let mut slot = lock();
    let live = slot.as_mut()?;
    let status = match live.0.try_wait() {
        Ok(None) => return None,
        Ok(Some(status)) => status,
        Err(_) => {
            slot.take();
            HOLD.store(true, Ordering::Relaxed);
            return Some(Err(
                "Screen pick cancelled. Press I to sample the screen.".into()
            ));
        }
    };
    let mut live = slot.take()?;
    let mut out = String::new();
    if let Some(mut stdout) = live.0.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    HOLD.store(true, Ordering::Relaxed);
    if !status.success() {
        return Some(Err(
            "Screen pick cancelled. Press I to sample the screen.".into()
        ));
    }
    Some(
        parse_hex_color(&out)
            .ok_or_else(|| "Screen pick cancelled. Press I to sample the screen.".into()),
    )
}

fn kill() {
    if let Some(Live(mut child)) = lock().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn sample_screen(fallback: Option<(i32, i32)>) -> Result<Rgba, String> {
    if let Ok(output) = Command::new("hyprctl").arg("cursorpos").output()
        && output.status.success()
        && let Some((x, y)) = parse_cursor_pos(&String::from_utf8_lossy(&output.stdout))
    {
        return sample_at(x, y);
    }
    if let Some((x, y)) = fallback {
        return sample_at(x, y);
    }
    Err("Could not read a screen pixel. Press I and click the pixel.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_pos_and_ppm_and_hex_parse() {
        assert_eq!(parse_cursor_pos("120, 40\n"), Some((120, 40)));
        assert_eq!(
            parse_hex_color("  #0a1b2c extra"),
            Some(Rgba::rgb(10, 27, 44))
        );
        let mut ppm = b"P6\n# note\n1 1\n255\n".to_vec();
        ppm.extend_from_slice(&[9, 8, 7]);
        assert_eq!(parse_ppm_pixel(&ppm), Some(Rgba::rgb(9, 8, 7)));
    }
}
