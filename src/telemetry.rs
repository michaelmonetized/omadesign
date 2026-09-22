//! Opt-in aggregate telemetry. No device IDs, user content, paths or raw errors.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://omadesign.app/api/telemetry";
static CONSENT: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
#[derive(Default, Serialize, Deserialize)]
struct Periods {
    day: String,
    week: String,
    month: String,
}
struct State {
    periods: Periods,
    counts: BTreeMap<(String, String), u32>,
    last_flush: Instant,
    mode: Option<crate::tools::Persona>,
    tool: Option<crate::tools::Tool>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            periods: Default::default(),
            counts: Default::default(),
            last_flush: Instant::now(),
            mode: None,
            tool: None,
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Count {
    pub period: String,
    pub metric: String,
    pub count: u32,
}
#[derive(Serialize, Deserialize)]
pub struct Batch {
    pub schema: u8,
    pub release: String,
    pub platform: String,
    pub counts: Vec<Count>,
}
fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}
fn config_path() -> std::path::PathBuf {
    crate::app::startup::preferences_path().with_file_name("telemetry-state.json")
}
pub fn set_consent(enabled: bool) {
    if CONSENT.swap(enabled, Ordering::SeqCst) == enabled {
        return;
    }
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    s.counts.clear();
    s.mode = None;
    s.tool = None;
    s.periods = if enabled {
        std::fs::read(config_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    } else {
        Default::default()
    };
    if !enabled {
        let _ = std::fs::remove_file(config_path());
    }
}
/// UTC calendar labels, plus Monday-based week start. No timestamps finer than a day.
fn periods_at(seconds: u64) -> (String, String, String) {
    fn date(days: i64) -> String {
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let mut y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        y += if m <= 2 { 1 } else { 0 };
        format!("{y:04}-{m:02}-{d:02}")
    }
    let days = (seconds / 86400) as i64;
    let day = date(days);
    let week = date(days - (days + 3).rem_euclid(7));
    let month = day[..7].to_owned();
    (day, week, month)
}
fn now_periods() -> (String, String, String) {
    periods_at(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
}
fn add(s: &mut State, period: String, metric: &str) {
    let count = s.counts.entry((period, metric.to_string())).or_default();
    *count = (*count + 1).min(10000);
}
pub fn activity(mode: crate::tools::Persona, tool: crate::tools::Tool, interacted: bool) {
    if !CONSENT.load(Ordering::Relaxed) {
        return;
    }
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    if !interacted {
        drop(s);
        flush(false);
        return;
    }
    let (day, week, month) = now_periods();
    if s.periods.day != day {
        add(&mut s, day.clone(), "active.day");
        s.periods.day = day.clone();
    }
    if s.periods.week != week {
        add(&mut s, week.clone(), "active.week");
        s.periods.week = week;
    }
    if s.periods.month != month {
        add(&mut s, month.clone(), "active.month");
        s.periods.month = month;
    }
    if s.mode != Some(mode) {
        add(
            &mut s,
            day.clone(),
            &format!("mode.{}", mode.name().to_ascii_lowercase()),
        );
        s.mode = Some(mode);
    }
    if s.tool != Some(tool) {
        add(&mut s, day, &format!("tool.{}", tool_code(tool)));
        s.tool = Some(tool);
    }
    drop(s);
    flush(false);
}
fn tool_code(t: crate::tools::Tool) -> &'static str {
    use crate::tools::Tool::*;
    match t {
        Select => "select",
        Node => "node",
        Pen => "pen",
        Pencil => "pencil",
        Rect => "rect",
        Ellipse => "ellipse",
        Polygon => "polygon",
        Star => "star",
        Line => "line",
        Text => "text",
        Gradient => "gradient",
        Eyedropper => "eyedropper",
        Trace => "trace",
        Brush => "brush",
        Eraser => "eraser",
        Fill => "fill",
        Clone => "clone",
        Heal => "heal",
        Smudge => "smudge",
        Crop => "crop",
        Marquee => "marquee",
        EllipseMarquee => "ellipse_marquee",
        Lasso => "lasso",
        Wand => "wand",
        Hand => "hand",
        Zoom => "zoom",
        Artboard => "artboard",
        Frame => "frame",
    }
}
pub fn count(metric: &'static str) {
    // Only fixed, audited categories can enter the payload.
    if !CONSENT.load(Ordering::Relaxed)
        || !matches!(
            metric,
            "feature.create"
                | "feature.edit"
                | "feature.group"
                | "feature.mask"
                | "feature.duplicate"
                | "feature.align"
                | "feature.import"
                | "feature.save"
                | "feature.export"
                | "feature.undo"
                | "feature.redo"
                | "feature.update"
                | "error.import"
                | "error.save"
                | "error.export"
                | "error.update"
                | "error.recovery"
                | "error.panic"
        )
    {
        return;
    }
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    add(&mut s, now_periods().0, metric);
}
fn batch(counts: Vec<Count>) -> Batch {
    Batch {
        schema: 1,
        release: env!("CARGO_PKG_VERSION").into(),
        platform: format!("linux-{}", std::env::consts::ARCH),
        counts,
    }
}
fn send(batch: Batch) {
    // Recheck immediately before transmission, including changes made by another window.
    if !consent_on_disk() {
        return;
    }
    if let Ok(body) = serde_json::to_string(&batch) {
        let _ = ureq::post(ENDPOINT)
            .timeout(Duration::from_secs(3))
            .set("Content-Type", "application/json")
            .set("User-Agent", "Omadesign anonymous counts/1")
            .send_string(&body);
    }
}
pub fn consent_on_disk() -> bool {
    crate::app::startup::Preferences::read(&crate::app::startup::preferences_path())
        .is_ok_and(|p| p.anonymous_usage)
}
pub fn flush(blocking: bool) {
    if !CONSENT.load(Ordering::Relaxed) {
        return;
    }
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    if s.counts.is_empty() || (!blocking && s.last_flush.elapsed() < Duration::from_secs(60)) {
        return;
    }
    let counts = std::mem::take(&mut s.counts)
        .into_iter()
        .map(|((period, metric), count)| Count {
            period,
            metric,
            count,
        })
        .collect();
    s.last_flush = Instant::now();
    // At-most-once best effort: no network retries that inflate active counts.
    if let Ok(bytes) = serde_json::to_vec(&s.periods) {
        let _ = crate::formats::write_atomic(&config_path(), &bytes);
    }
    drop(s);
    if blocking {
        send(batch(counts));
    } else {
        std::thread::spawn(move || {
            if CONSENT.load(Ordering::SeqCst) {
                send(batch(counts));
            }
        });
    }
}
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // The payload is deliberately passed only to the original local logger.
        count("error.panic");
        previous(info);
    }));
}
/// A separate waiting process survives native aborts. No core dump, backtrace or
/// panic payload is inspected or transmitted. CLI/export/screenshot runs bypass it.
#[cfg(unix)]
pub fn supervise(args: &[String]) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    const VERSION_PATH: &str = "OMADESIGN_SUPERVISOR_VERSION_PATH";
    if args.iter().any(|a| a == "--desktop-child") {
        if let Some(path) = std::env::var_os(VERSION_PATH).map(std::path::PathBuf::from) {
            // This ephemeral local file carries only a semantic version across
            // exec. It is never part of any transmitted event or installation ID.
            if path.starts_with(std::env::temp_dir())
                && path.file_name().is_some_and(|n| {
                    n.to_string_lossy()
                        .starts_with("omadesign-running-version-")
                })
            {
                let _ = std::fs::write(path, env!("CARGO_PKG_VERSION"));
            }
        }
        return None;
    }
    let version_path = std::env::temp_dir().join(format!(
        "omadesign-running-version-{}",
        crate::project::new_swap_id()
    ));
    let _ = std::fs::write(&version_path, env!("CARGO_PKG_VERSION"));
    let executable = std::env::current_exe().ok()?;
    let result = std::process::Command::new(executable)
        .args(args)
        .arg("--desktop-child")
        .env(VERSION_PATH, &version_path)
        .status()
        .ok()?;
    let release = std::fs::read_to_string(&version_path)
        .ok()
        .filter(|s| semver::Version::parse(s).is_ok());
    let _ = std::fs::remove_file(&version_path);
    let metric = match result.signal() {
        Some(11) => Some("crash.segv"),
        Some(6) => Some("crash.abort"),
        Some(7) => Some("crash.bus"),
        Some(4) => Some("crash.ill"),
        _ if result.code() == Some(101) => Some("crash.panic"),
        _ => None,
    };
    if let Some(metric) = metric
        && consent_on_disk()
    {
        let mut diagnostic = batch(vec![Count {
            period: now_periods().0,
            metric: metric.into(),
            count: 1,
        }]);
        if let Some(release) = release {
            diagnostic.release = release;
        }
        send(diagnostic);
    }
    Some(result.code().unwrap_or(1))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_periods_are_utc_and_weeks_start_monday() {
        assert_eq!(
            periods_at(0),
            ("1970-01-01".into(), "1969-12-29".into(), "1970-01".into())
        );
        assert_eq!(
            periods_at(1790035200),
            ("2026-09-22".into(), "2026-09-21".into(), "2026-09".into())
        );
    }
    #[test]
    fn payload_has_no_identity_or_freeform_context() {
        let b = batch(vec![Count {
            period: "2026-09-21".into(),
            metric: "crash.abort".into(),
            count: 1,
        }]);
        let v = serde_json::to_value(b).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 4);
        assert_eq!(v["counts"][0].as_object().unwrap().len(), 3);
        assert!(!crate::app::startup::Preferences::default().anonymous_usage);
    }
}
