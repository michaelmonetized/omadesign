//! One screen pixel, including windows outside this app.
use crate::color::Rgba;
use std::process::Command;

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
    Err("Could not read a screen pixel. Click the artwork, or drag off the window.".into())
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
