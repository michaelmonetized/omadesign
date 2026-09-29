use super::{
    Progress,
    models::{self, Model},
};
use ort::{
    session::{RunOptions, Session},
    value::Tensor,
};
use std::path::PathBuf;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

fn library_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("OMADESIGN_ORT_LIBRARY") {
        return Ok(path.into());
    }
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join("lib/libonnxruntime.so.1")); // Portable archive.
        paths.push(dir.join("../share/omadesign/lib/libonnxruntime.so.1")); // --prefix install.
    }
    if let Ok(data) = models::data_dir() {
        paths.push(data.join("lib/libonnxruntime.so.1"));
    }
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };
    paths.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/ml-downloads/onnxruntime-linux-{arch}-1.28.0/lib/libonnxruntime.so.1"
    )));
    paths.into_iter().find(|p| p.is_file()).ok_or_else(|| "ONNX Runtime is missing. Reinstall the complete Omadesign package (source builds: run scripts/prepare-ml-runtime.sh).".into())
}

pub fn initialize() -> Result<(), String> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| {
        let path = library_path()?;
        ort::init_from(path)
            .map_err(|e| format!("Cannot load ONNX Runtime: {e}"))?
            .with_name("Omadesign offline models")
            .commit();
        Ok(())
    })
    .clone()
}

pub struct Inference {
    session: Session,
    pub batch_size: usize,
    pub size: usize,
}
impl Inference {
    pub fn load(model: Model, progress: &dyn Progress) -> Result<Self, String> {
        progress.report("Loading offline model", 0, 1);
        initialize()?;
        let bytes = model.read(progress)?;
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 4));
        let session = Session::builder()
            .map_err(|e| e.to_string())?
            .with_intra_threads(threads)
            .map_err(|e| e.to_string())?
            .commit_from_memory(&bytes)
            .map_err(|e| e.to_string())?;
        let shape = session
            .inputs()
            .first()
            .and_then(|i| i.dtype().tensor_shape())
            .ok_or("Model has no image input")?;
        let size = model.input_size();
        if shape.len() != 4 || shape[1] != 3 || shape[2] != size as i64 || shape[3] != size as i64 {
            return Err("Unexpected model input shape".into());
        }
        // The pinned rembg exports have fixed N=1. Dynamic exports use chunks of
        // two (one for IS-Net) without changing the CPU/GPU inference path.
        let batch_size = if shape[0] < 0 {
            if size > 320 { 1 } else { 2 }
        } else if shape[0] == 1 {
            1
        } else {
            return Err("Unsupported model batch dimension".into());
        };
        progress.check()?;
        Ok(Self {
            session,
            batch_size,
            size,
        })
    }

    pub fn run(
        &mut self,
        input: Vec<f32>,
        n: usize,
        progress: &dyn Progress,
    ) -> Result<Vec<f32>, String> {
        let values = run_tensor(
            &mut self.session,
            input,
            [n, 3, self.size, self.size],
            [n, 1, self.size, self.size],
            progress,
        )?;
        // Pinned removal models end in sigmoid; retain their common scale.
        Ok(values.into_iter().map(|v| v.clamp(0., 1.)).collect())
    }
}

/// Shared cancellation and shape validation for both matting and super resolution.
pub(crate) fn run_tensor(
    session: &mut Session,
    input: Vec<f32>,
    shape: [usize; 4],
    expected: [usize; 4],
    progress: &dyn Progress,
) -> Result<Vec<f32>, String> {
    progress.check()?;
    let tensor = Tensor::from_array((shape, input)).map_err(|e| e.to_string())?;
    let options = RunOptions::new().map_err(|e| e.to_string())?;
    let finished = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        // ORT checks termination between operators; cancellation also works
        // during a single large HQ inference, not just between tiles.
        scope.spawn(|| {
            while !finished.load(Ordering::Acquire) {
                if progress.cancelled() {
                    let _ = options.terminate();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        // Also stop the cancellation watcher if ORT's wrapper panics.
        struct Finish<'a>(&'a AtomicBool);
        impl Drop for Finish<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _finish = Finish(&finished);
        let result = session
            .run_with_options(ort::inputs![tensor], &options)
            .map_err(|e| e.to_string())
            .and_then(|outputs| {
                let (shape, values) = outputs[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?;
                if shape.as_ref() != expected.map(|v| v as i64)
                    || values.iter().any(|v| !v.is_finite())
                {
                    return Err("Model returned invalid image data".into());
                }
                Ok(values.to_vec())
            });
        finished.store(true, Ordering::Release);
        result
    });
    progress.check()?;
    result
}
