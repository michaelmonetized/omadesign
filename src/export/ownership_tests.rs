use super::*;
use crate::deep_color::Depth;
use crate::document::{Layer, Pixels};
use crate::geom::Pt;
use crate::ml::NoProgress;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

fn document(depth: Depth) -> Document {
    let mut doc = Document::new("Output samples", 2., 1., 72.);
    doc.transparent = true;
    let samples = if depth == Depth::F32 {
        vec![1.25, 0.123456, -0.125, 1., 0.25, 0.75, 0.5, 0.8]
    } else {
        vec![
            1001. / 65535.,
            0.25,
            0.75,
            1.,
            1002. / 65535.,
            0.5,
            0.25,
            1.,
        ]
    };
    doc.layers = vec![Layer::placed_raster(
        "Authored",
        Pixels::from_samples(2, 1, depth, samples).unwrap(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    doc
}

fn retained(output: EncodedBytes) {
    let lease = Arc::downgrade(output.lease());
    assert!(output.lease().bytes() >= output.bytes());
    assert!(crate::resources::global().usage().0 >= output.bytes());
    let clone = output.clone();
    assert!(Arc::ptr_eq(&output, &clone));
    drop(output);
    assert!(lease.upgrade().is_some());
    drop(clone);
    assert!(lease.upgrade().is_none());
}

#[test]
fn admitted_native_outputs_preserve_precision_and_final_clone_ownership() {
    for depth in [Depth::U16, Depth::F32] {
        let doc = document(depth);
        let source = Source::Document(doc.clone());
        for format in [Format::Png, Format::Tiff, Format::Psd, Format::Psb] {
            if depth == Depth::F32 && format == Format::Png {
                continue;
            }
            let settings = Settings {
                format,
                ..Default::default()
            };
            let (bytes, _) = encode_admitted(&source, settings, &NoProgress).unwrap();
            let pixels = if matches!(format, Format::Psd | Format::Psb) {
                crate::formats::psd::decode(bytes.as_slice(), "Retained export")
                    .unwrap()
                    .document
                    .layers
                    .into_iter()
                    .find_map(|layer| layer.kind.pixels().cloned())
                    .unwrap()
            } else {
                crate::formats::decode_image(bytes.as_slice()).unwrap()
            };
            assert_eq!(pixels.native.as_ref().unwrap().depth, depth);
            let expected = doc.layers[0].kind.pixels().unwrap();
            for i in 0..2 {
                for (a, b) in pixels.sample(i).into_iter().zip(expected.sample(i)) {
                    assert!(
                        (a - b).abs() <= 1. / 65535.,
                        "{format:?} {depth:?}: {a} != {b}"
                    );
                }
            }
            retained(bytes);
        }
    }
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let pixels = Pixels::from_cmyk(
        1,
        1,
        Depth::U16,
        vec![1001. / 65535., 0.25, 0.5, 0.75, 1.],
        profile.clone(),
        1,
    )
    .unwrap();
    let output = crate::deep_color::tiff::encode_admitted(&pixels).unwrap();
    let decoded = crate::deep_color::tiff::decode(output.as_slice())
        .unwrap()
        .unwrap();
    assert_eq!(
        decoded.native.as_ref().unwrap().icc_profile.as_slice(),
        profile
    );
    assert!(
        (decoded.native.as_ref().unwrap().cmyk.as_ref().unwrap()[0] - 1001. / 65535.).abs() < 1e-6
    );
    retained(output);
    let doc = document(Depth::F32);
    let output = crate::formats::psd::encode_admitted(&doc, true).unwrap();
    retained(output.bytes);
}

struct CancelDuringEncoding(AtomicBool);
impl Progress for CancelDuringEncoding {
    fn report(&self, stage: &str, _: usize, _: usize) {
        if stage == "Encoding export" {
            self.0.store(true, Ordering::Release);
        }
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[test]
fn admitted_native_encoding_cancel_releases_working_storage_and_preserves_source() {
    let source = Source::Document(document(Depth::U16));
    let before = crate::resources::global().usage().0;
    let Source::Document(doc) = &source else {
        unreachable!()
    };
    let samples = doc.layers[0].kind.pixels().unwrap().samples().unwrap();
    assert!(
        encode_admitted(
            &source,
            Settings::default(),
            &CancelDuringEncoding(AtomicBool::new(false))
        )
        .is_err()
    );
    assert_eq!(crate::resources::global().usage().0, before);
    assert_eq!(
        doc.layers[0]
            .kind
            .pixels()
            .unwrap()
            .samples()
            .unwrap()
            .as_slice(),
        samples.as_slice()
    );
}

struct CancelWrite {
    cancelled: AtomicBool,
    output: Weak<crate::resources::Lease>,
}
impl Progress for CancelWrite {
    fn report(&self, stage: &str, done: usize, _: usize) {
        if stage == "Saving export" && done > 0 {
            assert!(self.output.upgrade().is_some());
            self.cancelled.store(true, Ordering::Release);
        }
    }
    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

fn directory() -> std::path::PathBuf {
    let path = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!(
            "native-output-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn admitted_queued_output_lives_through_real_cancelled_and_failed_file_writes() {
    let width = 512;
    let height = 128;
    let mut samples = crate::resources::global()
        .allocate_now::<f32, _>(
            (width * height * 4) as usize,
            &crate::deep_color::storage::check,
        )
        .unwrap();
    let mut state = 0x1836a7f1u32;
    for pixel in samples.as_mut_slice().chunks_exact_mut(4) {
        for channel in &mut pixel[..3] {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *channel = (state >> 16) as f32 / 65535.;
        }
        pixel[3] = 1.;
    }
    let pixels =
        Pixels::from_sample_allocation(width, height, Depth::U16, Arc::new(samples)).unwrap();
    let mut doc = Document::new("Queued output", width as f32, height as f32, 72.);
    doc.transparent = true;
    doc.layers = vec![Layer::placed_raster(
        "Pixels",
        pixels,
        Pt::ZERO,
        Pt::new(width as f32, height as f32),
    )];
    let (output, _) =
        encode_admitted(&Source::Document(doc), Settings::default(), &NoProgress).unwrap();
    assert!(
        output.len() > 256 * 1024,
        "Exercise cancellation between actual file chunks"
    );
    let lease = Arc::downgrade(output.lease());
    let root = directory();
    let destination = root.join("preserved.png");
    std::fs::write(&destination, b"accepted original").unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    send.send(output).unwrap();
    drop(send);
    assert!(lease.upgrade().is_some());
    let path = destination.clone();
    let weak = lease.clone();
    let result = std::thread::spawn(move || {
        let bytes: EncodedBytes = receive.recv().unwrap();
        write_atomic(
            &path,
            bytes.as_slice(),
            &CancelWrite {
                cancelled: AtomicBool::new(false),
                output: weak,
            },
        )
    })
    .join()
    .unwrap();
    assert!(result.is_err());
    assert!(lease.upgrade().is_none());
    assert_eq!(std::fs::read(&destination).unwrap(), b"accepted original");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    let output = crate::formats::psd::encode_admitted(&document(Depth::F32), false)
        .unwrap()
        .bytes;
    let lease = Arc::downgrade(output.lease());
    let blocked = root.join("existing-directory");
    std::fs::create_dir(&blocked).unwrap();
    assert!(write_atomic(&blocked, output.as_slice(), &NoProgress).is_err());
    assert!(lease.upgrade().is_some());
    drop(output);
    assert!(lease.upgrade().is_none());
    assert!(blocked.is_dir());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn admitted_save_and_cli_write_native_files_without_bare_vec_transfer() {
    let root = directory();
    for (format, depth) in [
        (Format::Png, Depth::U16),
        (Format::Tiff, Depth::F32),
        (Format::Psd, Depth::F32),
        (Format::Psb, Depth::U16),
    ] {
        let doc = document(depth);
        let path = root.join(format!("saved.{}", format.extension()));
        save(
            &Source::Document(doc.clone()),
            Settings {
                format,
                ..Default::default()
            },
            &path,
            &NoProgress,
        )
        .unwrap();
        let saved = std::fs::read(&path).unwrap();
        crate::formats::cli::export(&doc, &path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn admitted_failed_temp_creation_preserves_existing_files_and_releases_only_its_owner() {
    let root = directory();
    let destination = root.join("accepted.psd");
    let temporary = root.join("pre-existing.tmp");
    std::fs::write(&destination, b"accepted destination").unwrap();
    std::fs::write(&temporary, b"existing recovery evidence").unwrap();
    let output = crate::formats::psd::encode_admitted(&document(Depth::F32), false)
        .unwrap()
        .bytes;
    let lease = Arc::downgrade(output.lease());
    assert!(write_atomic_temp(&destination, &temporary, output.as_slice(), &NoProgress).is_err());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"accepted destination"
    );
    assert_eq!(
        std::fs::read(&temporary).unwrap(),
        b"existing recovery evidence"
    );
    assert!(lease.upgrade().is_some());
    drop(output);
    assert!(lease.upgrade().is_none());
    std::fs::remove_dir_all(root).unwrap();
}
