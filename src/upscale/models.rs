use crate::ml::{Progress, models};
use serde::Deserialize;
use std::{path::PathBuf, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Model {
    #[default]
    General,
    Quality,
    Native2,
    Illustration,
}

#[derive(Deserialize)]
struct Manifest {
    models: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    filename: String,
    size: u64,
    sha256: String,
}

impl Model {
    pub const ALL: [Self; 4] = [
        Self::General,
        Self::Quality,
        Self::Native2,
        Self::Illustration,
    ];
    fn artifact(self) -> &'static Artifact {
        static MANIFEST: OnceLock<Manifest> = OnceLock::new();
        &MANIFEST
            .get_or_init(|| {
                serde_json::from_str(include_str!("../../assets/models/upscale-manifest.json"))
                    .expect("pinned model manifest")
            })
            .models[self as usize]
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General · fast · bundled",
            Self::Quality => "Photo · high quality ×4",
            Self::Native2 => "Photo · native ×2",
            Self::Illustration => "Illustration · ×4",
        }
    }
    pub fn scale(self) -> u32 {
        if self == Self::Native2 { 2 } else { 4 }
    }
    pub fn filename(self) -> &'static str {
        &self.artifact().filename
    }
    pub fn size(self) -> u64 {
        self.artifact().size
    }
    pub fn sha256(self) -> &'static str {
        &self.artifact().sha256
    }
    pub fn path(self) -> Result<PathBuf, String> {
        Ok(models::data_dir()?.join("models").join(self.filename()))
    }
    pub fn present(self) -> bool {
        self == Self::General || self.path().is_ok_and(|p| p.is_file())
    }
    pub fn read(self, progress: &dyn Progress) -> Result<Vec<u8>, String> {
        progress.check()?;
        let bytes = if self == Self::General {
            include_bytes!("../../assets/models/realesr-general-x4v3.onnx").to_vec()
        } else {
            let path = self.path()?;
            if std::fs::metadata(&path)
                .map_err(|_| "Download this model first".to_owned())?
                .len()
                != self.size()
            {
                return Err("Model size mismatch. Download a verified copy again.".into());
            }
            std::fs::read(path).map_err(|e| e.to_string())?
        };
        models::verify(&bytes, self.size(), self.sha256())?;
        progress.check()?;
        Ok(bytes)
    }
    pub fn download(self, progress: &dyn Progress) -> Result<(), String> {
        if self == Self::General {
            return Ok(());
        }
        progress.check()?;
        let url = format!(
            "https://github.com/michaelmonetized/omadesign/releases/download/upscale-models-v1/{}",
            self.filename()
        );
        let response = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_secs(15))
            .build()
            .get(&url)
            .call()
            .map_err(|e| e.to_string())?;
        models::install_verified(
            response.into_reader(),
            &self.path()?,
            self.size(),
            self.sha256(),
            progress,
        )
    }
}
