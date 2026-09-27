use super::Progress;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    U2NetP,
    U2Net,
    IsNet,
}

impl Model {
    pub const ALL: [Self; 3] = [Self::U2NetP, Self::U2Net, Self::IsNet];
    pub fn label(self) -> &'static str {
        match self {
            Self::U2NetP => "Fast · bundled",
            Self::U2Net => "High quality · U²-Net",
            Self::IsNet => "High quality · IS-Net",
        }
    }
    pub fn filename(self) -> &'static str {
        match self {
            Self::U2NetP => "u2netp.onnx",
            Self::U2Net => "u2net.onnx",
            Self::IsNet => "isnet-general-use.onnx",
        }
    }
    pub fn size(self) -> u64 {
        match self {
            Self::U2NetP => 4_574_861,
            Self::U2Net => 175_997_641,
            Self::IsNet => 178_648_008,
        }
    }
    pub fn sha256(self) -> &'static str {
        match self {
            Self::U2NetP => "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
            Self::U2Net => "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491",
            Self::IsNet => "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
        }
    }
    pub fn input_size(self) -> usize {
        if self == Self::IsNet { 1024 } else { 320 }
    }
    pub fn normalization(self) -> ([f32; 3], [f32; 3]) {
        if self == Self::IsNet {
            ([0.5; 3], [1.; 3])
        } else {
            ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225])
        }
    }
    pub fn path(self) -> Result<PathBuf, String> {
        Ok(data_dir()?.join("models").join(self.filename()))
    }
    /// Availability only; every load verifies the entire file off the UI thread.
    pub fn present(self) -> bool {
        self == Self::U2NetP || self.path().is_ok_and(|p| p.is_file())
    }
    pub fn read(self, progress: &dyn Progress) -> Result<Vec<u8>, String> {
        progress.check()?;
        let bytes = if self == Self::U2NetP {
            include_bytes!("../../assets/models/u2netp.onnx").to_vec()
        } else {
            let path = self.path()?;
            if fs::metadata(&path)
                .map_err(|_| "Download this model first using the Download button".to_string())?
                .len()
                != self.size()
            {
                return Err("Model size mismatch. Download a verified copy again.".into());
            }
            fs::read(path).map_err(|e| e.to_string())?
        };
        verify(&bytes, self.size(), self.sha256())?;
        progress.check()?;
        Ok(bytes)
    }
    /// Called only in response to the explicit Download action.
    pub fn download(self, progress: &dyn Progress) -> Result<(), String> {
        if self == Self::U2NetP {
            return Ok(());
        }
        let path = self.path()?;
        let url = format!(
            "https://github.com/danielgatis/rembg/releases/download/v0.0.0/{}",
            self.filename()
        );
        progress.check()?;
        let response = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_secs(15))
            .build()
            .get(&url)
            .call()
            .map_err(|e| e.to_string())?;
        install_verified(
            response.into_reader(),
            &path,
            self.size(),
            self.sha256(),
            progress,
        )
    }
}

pub fn data_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|p| Path::new(p).is_absolute()) {
        return Ok(PathBuf::from(path).join("omadesign"));
    }
    std::env::var_os("HOME")
        .map(|p| PathBuf::from(p).join(".local/share/omadesign"))
        .ok_or_else(|| "No user data directory is available".into())
}

fn verify(bytes: &[u8], size: u64, sha: &str) -> Result<(), String> {
    if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(bytes)) != sha {
        Err("Model verification failed (size or SHA-256). The model was rejected.".into())
    } else {
        Ok(())
    }
}

fn install_verified(
    mut source: impl Read,
    path: &Path,
    size: u64,
    sha: &str,
    progress: &dyn Progress,
) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Invalid model path")?).map_err(|e| e.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let part = path.with_extension(format!("{}.{}.part", std::process::id(), nonce));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            progress.check()?;
            let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            done += n as u64;
            if done > size {
                return Err("Model exceeds its pinned size; download rejected".into());
            }
            file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            hasher.update(&buffer[..n]);
            progress.report("Downloading model", done as usize, size as usize);
        }
        if done != size || format!("{:x}", hasher.finalize()) != sha {
            return Err(
                "Model verification failed (size or SHA-256). The download was rejected.".into(),
            );
        }
        progress.check()?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&part, path).map_err(|e| e.to_string())?;
        File::open(path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = fs::remove_file(part);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_model_matches_pin() {
        Model::U2NetP.read(&super::super::NoProgress).unwrap();
    }
    #[test]
    fn rejects_corrupt_or_cancelled_download_without_replacing_model() {
        let root = std::env::temp_dir().join(format!("oma-model-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("model.onnx");
        fs::write(&path, b"previous verified model").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"valid"));
        for content in [b"wrong".as_slice(), b"too long", b"short".get(..2).unwrap()] {
            assert!(install_verified(content, &path, 5, &hash, &super::super::NoProgress).is_err());
        }
        struct Cancel;
        impl Progress for Cancel {
            fn report(&self, _: &str, _: usize, _: usize) {}
            fn cancelled(&self) -> bool {
                true
            }
        }
        assert!(install_verified(b"valid".as_slice(), &path, 5, &hash, &Cancel).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"previous verified model");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        install_verified(
            b"valid".as_slice(),
            &path,
            5,
            &hash,
            &super::super::NoProgress,
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"valid");
        fs::remove_dir_all(root).unwrap();
    }
}
