//! A folder is a thumbnail gallery, not a collection of full camera images.
//! Decode one source at a time, release it immediately, and bound both retained
//! pixels and the completed-worker queue regardless of folder size.
use super::*;

const PIXEL_BUDGET: usize = 96 * 1024 * 1024;
const MAX_THUMB_EDGE: u32 = 256;
// A cancelled decoder may still be finishing its current image. Serialize the
// expensive decode so rapid folder changes cannot accumulate camera buffers.
static DECODE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Default)]
pub struct FolderPreview {
    pub pixels: Option<RgbaImage>,
    pub error: Option<String>,
}

struct PreviewJob {
    receiver: mpsc::Receiver<(usize, Result<RgbaImage, String>)>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct FolderGallery {
    pub previews: Vec<FolderPreview>,
    pub textures: BTreeMap<usize, egui::TextureHandle>,
    job: Option<PreviewJob>,
    completed: usize,
}

fn thumbnail_edge(count: usize) -> u32 {
    ((PIXEL_BUDGET / count.max(1) / 4) as f64)
        .sqrt()
        .floor()
        .clamp(1.0, MAX_THUMB_EDGE as f64) as u32
}

impl FolderGallery {
    pub fn load(&mut self, files: &[(String, String)]) {
        self.cancel();
        self.previews = (0..files.len()).map(|_| FolderPreview::default()).collect();
        self.textures.clear();
        self.completed = 0;
        if files.is_empty() {
            return;
        }
        let paths = files
            .iter()
            .map(|(_, path)| PathBuf::from(path))
            .collect::<Vec<_>>();
        let edge = thumbnail_edge(paths.len());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(2);
        match std::thread::Builder::new()
            .name("photo-thumbnails".into())
            .spawn(move || {
                for (index, path) in paths.into_iter().enumerate() {
                    if worker_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let pixels = {
                        let _guard = DECODE_LOCK
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if worker_cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        PhotoImage::load(&path).map(|image| image.render_thumbnail(edge))
                    };
                    if worker_cancel.load(Ordering::Relaxed)
                        || sender.send((index, pixels)).is_err()
                    {
                        break;
                    }
                }
            }) {
            Ok(_) => self.job = Some(PreviewJob { receiver, cancel }),
            Err(error) => {
                for preview in &mut self.previews {
                    preview.error = Some(format!("Could not load thumbnail: {error}"));
                }
                self.completed = self.previews.len();
            }
        }
    }

    pub fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
            // Dropping the receiver also releases a worker blocked on send.
        }
    }

    pub fn is_loading(&self) -> bool {
        self.job.is_some()
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.completed, self.previews.len())
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        let mut disconnected = false;
        for _ in 0..8 {
            match job.receiver.try_recv() {
                Ok((index, result)) => {
                    if let Some(preview) = self.previews.get_mut(index) {
                        match result {
                            Ok(pixels) => preview.pixels = Some(pixels),
                            Err(error) => preview.error = Some(error),
                        }
                        self.completed += 1;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            self.job = None;
            for preview in &mut self.previews {
                if preview.pixels.is_none() && preview.error.is_none() {
                    preview.error =
                        Some("Thumbnail loading stopped. Reopen the folder to retry.".into());
                }
            }
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
    }

    pub fn texture(&mut self, ctx: &egui::Context, index: usize) -> Option<&egui::TextureHandle> {
        if !self.textures.contains_key(&index) {
            let pixels = self.previews.get(index)?.pixels.as_ref()?;
            let texture = ctx.load_texture(
                format!("photo-folder-{index}"),
                egui::ColorImage::from_rgba_unmultiplied(
                    [pixels.w as usize, pixels.h as usize],
                    &pixels.data,
                ),
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(index, texture);
        }
        self.textures.get(&index)
    }
}

impl Drop for FolderGallery {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_preview_pixels_are_bounded_for_large_folders() {
        for count in [1, 100, 1_000, MAX_FOLDER_FILES] {
            let edge = thumbnail_edge(count) as usize;
            assert!(edge <= MAX_THUMB_EDGE as usize);
            assert!(count * edge * edge * 4 <= PIXEL_BUDGET);
        }
    }
}
