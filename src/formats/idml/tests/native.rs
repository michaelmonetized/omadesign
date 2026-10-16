use super::*;
use crate::{deep_color::Depth, document::Pixels};
use std::io::{Cursor, Read, Write};

#[test]
fn placed_float_cmyk_tiff_retains_native_storage_and_import_budget() {
    let profile = include_bytes!("../../psd/fixtures/fogra39l-cc0.icc");
    let inks = vec![0.1234567, 0.2345678, 0.3456789, 0.4567891, 1.];
    let pixels = Pixels::from_cmyk(1, 1, Depth::F32, inks.clone(), profile.to_vec(), 1).unwrap();
    let encoded = crate::deep_color::tiff::encode_admitted(&pixels).unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(FIXTURE)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for i in 0..input.len() {
        let mut part = input.by_index(i).unwrap();
        output.start_file(part.name(), options).unwrap();
        if part.name() == "Links/mark.png" {
            output.write_all(encoded.as_slice()).unwrap();
        } else {
            let mut bytes = Vec::new();
            part.read_to_end(&mut bytes).unwrap();
            output.write_all(&bytes).unwrap();
        }
    }
    let package = output.finish().unwrap().into_inner();
    let (doc, _) = read(&package, "Native placed image").unwrap();
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    for document in [&doc, &reopened] {
        let pixels = document
            .layers
            .iter()
            .find_map(|layer| layer.kind.pixels())
            .unwrap();
        pixels.validate_native().unwrap();
        let native = pixels.native.as_ref().unwrap();
        assert_eq!(native.depth, Depth::F32);
        assert_eq!(native.cmyk.as_ref().unwrap().as_slice(), inks);
        assert_eq!(native.icc_profile.as_slice(), profile);
        assert_eq!(native.intent, 1);
    }
    let allowance = profile.len() / 2;
    super::super::read::read_with_budget(FIXTURE, "Small byte image", &|_| None, allowance)
        .unwrap();
    let error = super::super::read::read_with_budget(
        &package,
        "Native image exceeds import allowance",
        &|_| None,
        allowance,
    )
    .unwrap_err();
    assert!(error.contains("allocation budget"), "{error}");
}
