use crate::geom::Pt;
use crate::photo::{self, Histogram, PhotoImage, RgbaImage};
use eframe::egui;

pub struct PhotoSession {
    pub images: Vec<PhotoImage>,
    pub selected: Option<usize>,
    pub folder: String,
    pub folder_files: Vec<(String, String)>,
    pub view_scale: f32,
    pub view_offset: egui::Vec2,
    pub fit_scale: f32,
    pub show_original: bool,
    pub crop_drag: Option<(Pt, Pt)>,
    pub adjusted: Option<RgbaImage>,
    pub hists: [Histogram; 4],
    pub tex: Option<egui::TextureHandle>,
    pub orig_tex: Option<egui::TextureHandle>,
    pub thumbs: Vec<egui::TextureHandle>,
    pub sel_version: u64,
    pub built_version: u64,
    pub orig_built: u64,
    pub dirty: bool,
    pub samples_loaded: bool,
    pub status: String,
}

impl Default for PhotoSession {
    fn default() -> Self {
        Self::new()
    }
}

impl PhotoSession {
    pub fn new() -> Self {
        Self {
            images: vec![],
            selected: None,
            folder: String::new(),
            folder_files: vec![],
            view_scale: 1.0,
            view_offset: egui::Vec2::ZERO,
            fit_scale: 1.0,
            show_original: false,
            crop_drag: None,
            adjusted: None,
            hists: Default::default(),
            tex: None,
            orig_tex: None,
            thumbs: vec![],
            sel_version: 0,
            built_version: 0,
            orig_built: u64::MAX,
            dirty: true,
            samples_loaded: false,
            status: "Open a folder, drop photos, or load samples".into(),
        }
    }

    pub fn selected(&self) -> Option<&PhotoImage> {
        self.selected.and_then(|i| self.images.get(i))
    }

    pub fn selected_mut(&mut self) -> Option<&mut PhotoImage> {
        self.selected.and_then(|i| self.images.get_mut(i))
    }

    pub fn select_image(&mut self, index: usize) {
        if index < self.images.len() {
            self.selected = Some(index);
            self.view_scale = 1.0;
            self.view_offset = egui::Vec2::ZERO;
            self.sel_version += 1;
            self.dirty = true;
        }
    }

    pub fn import_file(&mut self, path: &std::path::Path) {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        match photo::load_file(path) {
            Some(full) => {
                self.images.push(PhotoImage::from_full(name, full));
                self.select_image(self.images.len() - 1);
                self.status = "imported".into();
            }
            None => self.status = format!("could not open {}", path.display()),
        }
    }

    pub fn import_bytes(&mut self, name: String, bytes: &[u8]) {
        match photo::decode_bytes(bytes) {
            Some(full) => {
                let name = name.split('/').next_back().unwrap_or(&name).to_string();
                self.images.push(PhotoImage::from_full(name, full));
                self.select_image(self.images.len() - 1);
                self.status = "imported".into();
            }
            None => self.status = "failed to decode image".into(),
        }
    }

    pub fn import_samples(&mut self) {
        for kind in 0..3 {
            let (name, full) = photo::sample_photo(kind);
            self.images.push(PhotoImage::from_full(name, full));
        }
        self.select_image(0);
        self.samples_loaded = true;
        self.status = "sample photos loaded".into();
    }

    pub fn set_folder(&mut self, path: &str) {
        let mut entries = vec![];
        if let Ok(read) = std::fs::read_dir(path) {
            for e in read.flatten() {
                let p = e.path();
                if p.is_file()
                    && let Some(ext) = p.extension().and_then(|s| s.to_str())
                    && matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff"
                    )
                {
                    entries.push((
                        p.file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        p.to_string_lossy().to_string(),
                    ));
                }
            }
        }
        entries.sort();
        self.folder = path.to_string();
        self.folder_files = entries;
    }

    pub fn rebuild(&mut self) {
        let Some(img) = self.selected() else {
            return;
        };
        let src = if img.preview.w * img.preview.h > 0 {
            &img.preview
        } else {
            &img.full
        };
        let out = photo::develop(src, &img.develop);
        self.hists = photo::histograms(&out);
        self.adjusted = Some(out);
        self.dirty = false;
    }
}
