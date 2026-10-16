use super::*;
use crate::{
    deep_color::{
        Depth,
        cms::{DocumentColor, Model},
        storage,
    },
    document::{Layer, Pixels},
    geom::Pt,
};
use std::sync::Arc;

/// Build the original Live raster fixture independently of current document defaults.
/// Args: document name, canvas dimensions and resolution. Returns the original opaque raster baseline.
pub(crate) fn legacy_fixture(
    name: impl Into<String>,
    width: f32,
    height: f32,
    dpi: f32,
) -> Document {
    let mut doc = Document::new(name, width, height, dpi);
    doc.layers[0] = Layer::raster(
        "Background",
        width.round().max(1.) as u32,
        height.round().max(1.) as u32,
    )
    .unwrap();
    doc.layers[0]
        .kind
        .pixels_mut()
        .unwrap()
        .bytes_mut()
        .unwrap()
        .fill(255);
    doc
}

pub(crate) fn fixture() -> Document {
    let mut doc = legacy_fixture("Native live", 32., 32., 72.);
    doc.color = DocumentColor {
        model: Model::Cmyk,
        depth: Depth::F32,
        icc_profile: vec![],
        intent: 0,
    };
    let mut rgb = Pixels::from_samples(
        2,
        1,
        Depth::F32,
        vec![
            -0.125,
            0.5,
            f32::from_bits(0x3f000001),
            0.375,
            3.25,
            -0.0,
            0.25,
            1.,
        ],
    )
    .unwrap();
    rgb.assign_rgb_profile(
        include_bytes!("../formats/psd/fixtures/srgb-cc0.icc").to_vec(),
        0,
    )
    .unwrap();
    let inks = Pixels::from_cmyk(
        1,
        1,
        Depth::U16,
        vec![0.1, 0.2, 0.3, 0.4, 0.625],
        include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec(),
        0,
    )
    .unwrap();
    doc.layers
        .push(Layer::placed_raster("RGB", rgb, Pt::ZERO, Pt::new(2., 1.)));
    doc.layers.push(Layer::placed_raster(
        "CMYK",
        inks,
        Pt::ZERO,
        Pt::new(1., 1.),
    ));
    doc
}
pub(crate) fn assert_native_equal(a: &Document, b: &Document) {
    assert_eq!(a.color, b.color);
    for (a, b) in a.layers.iter().zip(&b.layers) {
        for (a, b) in a
            .kind
            .pixels()
            .into_iter()
            .chain(a.mask.as_ref())
            .zip(b.kind.pixels().into_iter().chain(b.mask.as_ref()))
        {
            assert_eq!((a.w, a.h), (b.w, b.h));
            assert_eq!(a.data, b.data);
            assert_eq!(a.native, b.native);
            if let (Some(a), Some(b)) = (&a.native, &b.native) {
                assert_eq!(
                    a.samples.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    b.samples.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                );
                assert_eq!(
                    a.cmyk
                        .as_ref()
                        .map(|v| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
                    b.cmyk
                        .as_ref()
                        .map(|v| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>())
                );
            }
        }
    }
}
pub(crate) fn edit(doc: &mut Document, role: &str) {
    if role == "Alice" {
        let pixels = doc.layers[2].kind.pixels_mut().unwrap();
        let native = pixels.native.as_mut().unwrap();
        let mut copy = native.samples.try_clone_now(&storage::check).unwrap();
        copy.as_mut_slice()[0] = -0.25;
        native.samples = Arc::new(copy);
        pixels.touch();
    } else {
        let pixels = doc.layers[3].kind.pixels_mut().unwrap();
        let native = pixels.native.as_ref().unwrap();
        let mut copy = native
            .cmyk
            .as_ref()
            .unwrap()
            .try_clone_now(&storage::check)
            .unwrap();
        copy.as_mut_slice()[0] = 0.125;
        *pixels = Pixels::from_cmyk_allocation(
            1,
            1,
            native.depth,
            Arc::new(copy),
            native.icc_profile.as_slice().to_vec(),
            native.intent,
        )
        .unwrap();
    }
}

#[test]
fn native_pixels_color_masks_and_save_reopen_retain_exact_authored_bits() {
    let mut doc = fixture();
    doc.color.icc_profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    doc.layers[2].mask = Some(doc.layers[2].kind.pixels().unwrap().clone());
    let seed = snapshot(&doc).unwrap();
    let replica = Replica::new("native".into(), "a".repeat(64), seed);
    let merged = replica.document(&doc).unwrap();
    assert_native_equal(&doc, &merged);
    assert_eq!(
        merged.layers[2].kind.pixels().unwrap().samples().unwrap()[0].to_bits(),
        (-0.125f32).to_bits()
    );
    let mut saved = merged;
    saved.collaboration = Some(replica);
    let encoded = crate::project::encode(&saved).unwrap();
    let independently_parsed: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(independently_parsed["doc"]["color"]["intent"], 0);
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_native_equal(&doc, &reopened);
    assert_native_equal(
        &doc,
        &reopened
            .collaboration
            .as_ref()
            .unwrap()
            .document(&reopened)
            .unwrap(),
    );
}

#[test]
fn native_two_peers_reconnect_and_undo_their_own_atomic_pixel_edits() {
    let doc = fixture();
    let seed = snapshot(&doc).unwrap();
    let mut a = Replica::new("native".into(), "a".repeat(64), seed.clone());
    let mut b = Replica::new("native".into(), "b".repeat(64), seed);
    let mut ad = doc.clone();
    let mut bd = doc.clone();
    edit(&mut ad, "Alice");
    edit(&mut bd, "Bob");
    a.record(&ad).unwrap();
    b.record(&bd).unwrap();
    a.receive(b.changes.clone()).unwrap();
    b.receive(a.changes.clone()).unwrap();
    let merged = a.document(&doc).unwrap();
    assert_native_equal(&merged, &b.document(&doc).unwrap());
    assert_eq!(
        merged.layers[2].kind.pixels().unwrap().samples().unwrap()[0],
        -0.25
    );
    assert_eq!(
        merged.layers[3]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap()[0],
        0.125
    );
    assert!(a.undo());
    b.receive(a.changes.clone()).unwrap();
    let undone = b.document(&doc).unwrap();
    assert_eq!(
        undone.layers[2].kind.pixels().unwrap().samples().unwrap()[0],
        -0.125
    );
    assert_eq!(
        undone.layers[3]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .cmyk
            .as_ref()
            .unwrap()[0],
        0.125
    );
    assert!(a.redo());
    b.receive(a.changes.clone()).unwrap();
    assert_native_equal(&merged, &b.document(&doc).unwrap());
}

#[test]
fn native_protocol_refuses_legacy_roundtrips_but_preserves_neutral_seeds() {
    let native = fixture();
    let state = Replica::new("native".into(), "a".repeat(64), snapshot(&native).unwrap());
    let before = serde_json::to_vec(&state).unwrap();
    assert!(
        state
            .document_for_protocol(&native, 1)
            .unwrap_err()
            .contains("protocol 2")
    );
    assert_eq!(before, serde_json::to_vec(&state).unwrap());
    let neutral = Document::new("Legacy", 1., 1., 72.);
    let mut seed = snapshot(&neutral).unwrap();
    seed.remove("/color");
    let old = Replica::new("legacy".into(), "a".repeat(64), seed);
    assert_eq!(old.required_protocol(), 1);
    let materialized = old.document_for_protocol(&neutral, 1).unwrap();
    assert_eq!(materialized.color, DocumentColor::default());
}

#[test]
fn native_aggregate_wire_quota_precedes_allocation_and_counts_inks_profiles_and_display() {
    let doc = fixture();
    let cells = snapshot(&doc).unwrap();
    let expected = cells
        .values()
        .filter(|c| matches!(c, Cell::Pixels { .. }))
        .map(|c| pixels::size(c).unwrap())
        .sum::<u64>();
    assert!(expected > 122152);
    let mut placeholder: Document = serde_json::from_value(inflate("", &cells).unwrap()).unwrap();
    let bytes = serde_json::to_vec(&placeholder).unwrap();
    let calls = std::cell::Cell::new(0);
    let check = || {
        calls.set(calls.get() + 1);
        Ok(())
    };
    assert!(
        unpack_pixels_checked(&mut placeholder, &cells, expected - 1, &check)
            .unwrap_err()
            .contains("total pixel budget")
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(bytes, serde_json::to_vec(&placeholder).unwrap());
    unpack_pixels_checked(&mut placeholder, &cells, expected, &check).unwrap();
    assert_native_equal(&doc, &placeholder);
    let mut oversized = cells.clone();
    let path = format!("/@entities/{}/kind/Raster/pixels", doc.layers[2].id);
    if let Some(Cell::Pixels { w, h, .. }) = oversized.get_mut(&path) {
        *w = 8192;
        *h = 8192;
    }
    assert!(preflight_pixels(&inflate("", &oversized).unwrap(), &oversized, PIXEL_BUDGET).is_err());
}

#[test]
fn native_malformed_payload_and_mid_decode_cancellation_preserve_source_bytes() {
    let doc = fixture();
    let cells = snapshot(&doc).unwrap();
    let path = format!("/@entities/{}/kind/Raster/pixels", doc.layers[2].id);
    let original = crate::project::encode(&doc).unwrap();
    for (key, value) in [
        ("samples", json!("bad frame")),
        ("intent", json!(4)),
        ("icc_bytes", json!(1)),
        ("unknown", json!(true)),
    ] {
        let mut state = Replica::new("native".into(), "a".repeat(64), cells.clone());
        if let Some(Cell::Pixels {
            native: Some(n), ..
        }) = state.base.get_mut(&path)
        {
            n[key] = value;
        }
        let state_bytes = serde_json::to_vec(&state).unwrap();
        assert!(state.document(&doc).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), state_bytes);
        assert_eq!(crate::project::encode(&doc).unwrap(), original);
    }
    let mut placeholder: Document = serde_json::from_value(inflate("", &cells).unwrap()).unwrap();
    let before = serde_json::to_vec(&placeholder).unwrap();
    let calls = std::cell::Cell::new(0);
    let cancel = || {
        calls.set(calls.get() + 1);
        if calls.get() > 12 {
            Err("Cancelled live native decode".into())
        } else {
            Ok(())
        }
    };
    assert!(
        unpack_pixels_checked(&mut placeholder, &cells, PIXEL_BUDGET, &cancel)
            .unwrap_err()
            .contains("Cancelled")
    );
    assert_eq!(before, serde_json::to_vec(&placeholder).unwrap());
    assert_eq!(crate::project::encode(&doc).unwrap(), original);
}

#[test]
fn native_admission_refusal_runs_in_an_independent_process() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "collaboration::native_tests::allocation_pressure_child",
            "--ignored",
            "--nocapture",
        ])
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
#[ignore = "launched in isolation by native_admission_refusal_runs_in_an_independent_process"]
fn allocation_pressure_child() {
    let doc = fixture();
    let cells = snapshot(&doc).unwrap();
    let mut placeholder: Document = serde_json::from_value(inflate("", &cells).unwrap()).unwrap();
    let before = serde_json::to_vec(&placeholder).unwrap();
    let pool = crate::resources::global();
    let mut held = vec![];
    while let Ok(lease) = pool.reserve_now(32 * 1024 * 1024, &storage::check) {
        held.push(lease);
    }
    let usage = pool.usage().0;
    let start = std::time::Instant::now();
    let error =
        unpack_pixels_checked(&mut placeholder, &cells, PIXEL_BUDGET, &storage::check).unwrap_err();
    assert!(error.contains("memory"), "{error}");
    assert!(start.elapsed().as_secs() < 2);
    assert_eq!(before, serde_json::to_vec(&placeholder).unwrap());
    assert_eq!(pool.usage().0, usage);
    drop(held);
    unpack_pixels_checked(&mut placeholder, &cells, PIXEL_BUDGET, &storage::check).unwrap();
    assert_native_equal(&doc, &placeholder);
}
