//! One worker decodes the hovered document once and renders only the latest
//! requested time. Scrubbing never decodes or composes on the event loop.
use super::catalog::{self, DocumentEntry};
use eframe::egui::{self, TextureHandle};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Clone, PartialEq, Eq)]
struct Key(PathBuf, u128, u64, u8);
struct State {
    tx: mpsc::Sender<(Key, bool)>,
    rx: mpsc::Receiver<(Key, Result<egui::ColorImage, String>)>,
    requested: Option<Key>,
    cache: VecDeque<(Key, TextureHandle)>,
    failed: Option<Key>,
    alive: Arc<AtomicBool>,
}
type Shared = Arc<Mutex<State>>;

pub(super) fn image(
    ctx: &egui::Context,
    entry: &DocumentEntry,
    fraction: f32,
) -> Option<TextureHandle> {
    let shared = ctx.data_mut(|d| {
        let id = egui::Id::new("welcome-motion-scrub");
        if let Some(state) = d.get_temp::<Shared>(id)
            && state.lock().is_ok_and(|s| s.alive.load(Ordering::Relaxed))
        {
            return state;
        }
        let (tx, requests) = mpsc::channel::<(Key, bool)>();
        let (completed, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let alive = Arc::new(AtomicBool::new(true));
        let worker_alive = alive.clone();
        std::thread::spawn(move || {
            let mut loaded: Option<(Key, crate::document::Document)> = None;
            while let Ok(mut request) = requests.recv_timeout(std::time::Duration::from_secs(10)) {
                while let Ok(latest) = requests.try_recv() {
                    request = latest;
                }
                let (key, recovered) = request;
                let result = (|| {
                    if loaded
                        .as_ref()
                        .is_none_or(|(old, _)| old.0 != key.0 || old.1 != key.1 || old.2 != key.2)
                    {
                        loaded = None;
                        let doc = crate::brand::load_full_preview_document(&key.0, recovered)?;
                        loaded = Some((key.clone(), doc));
                    }
                    let doc = &loaded.as_ref().unwrap().1;
                    let time = f32::from(key.3) / 60. * doc.motion.duration.max(0.);
                    let pixels = catalog::render_document_at(doc, Some(time))?;
                    Ok(egui::ColorImage::from_rgba_unmultiplied(
                        [pixels.w as usize, pixels.h as usize],
                        &pixels.data,
                    ))
                })();
                if completed.send((key, result)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
            worker_alive.store(false, Ordering::Relaxed);
        });
        let state = Arc::new(Mutex::new(State {
            tx,
            rx,
            requested: None,
            cache: VecDeque::new(),
            failed: None,
            alive,
        }));
        d.insert_temp(id, state.clone());
        state
    });
    let key = Key(
        entry.path.clone(),
        entry.modified_ns,
        entry.size,
        (fraction.clamp(0., 1.) * 60.).round() as u8,
    );
    let mut state = shared.lock().ok()?;
    while let Ok((key, result)) = state.rx.try_recv() {
        match result {
            Ok(pixels) => {
                let texture =
                    ctx.load_texture("motion-scrub", pixels, egui::TextureOptions::LINEAR);
                state.cache.push_back((key, texture));
                while state.cache.len() > 24 {
                    state.cache.pop_front();
                }
            }
            Err(_) => state.failed = Some(key),
        }
    }
    if let Some((_, texture)) = state.cache.iter().find(|(cached, _)| cached == &key) {
        return Some(texture.clone());
    }
    if state.requested.as_ref() != Some(&key) && state.failed.as_ref() != Some(&key) {
        let _ = state.tx.send((key.clone(), entry.recovered));
        state.requested = Some(key.clone());
    }
    state
        .cache
        .iter()
        .rev()
        .find(|(old, _)| old.0 == key.0 && old.1 == key.1 && old.2 == key.2)
        .map(|(_, t)| t.clone())
}
