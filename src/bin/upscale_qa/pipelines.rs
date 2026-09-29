use omadesign::{
    background_removal::{self as removal, Source},
    export,
    ml::{NoProgress, models::Model as Matte},
    photo::{PhotoImage, RgbaImage},
    upscale::{
        self,
        cutout::{self, Workflow},
    },
};
use std::{path::Path, sync::Arc};

pub fn run(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output)?;
    let img = image::open(input)?
        .resize_exact(65, 43, image::imageops::FilterType::CatmullRom)
        .to_rgba8();
    let mut photo = PhotoImage::from_full(
        "developed source".into(),
        RgbaImage {
            w: 65,
            h: 43,
            data: img.clone().into_raw(),
        },
    );
    photo.develop.rotate = 90;
    photo.develop.crop = Some([0.1, 0.2, 0.9, 0.8]);
    photo.develop.exposure = 0.4;
    let source = export::Source::Photo(photo);
    let mut reports = vec![];
    for factor in [2., 4., 5., 2.5] {
        for format in [
            export::Format::Png,
            export::Format::Jpeg,
            export::Format::Tiff,
        ] {
            let settings = export::Settings {
                format,
                ai: Some(upscale::Settings {
                    factor,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let path = output.join(format!("photo-{factor}.{}", format.extension()));
            export::save(&source, settings, &path, &NoProgress)?;
            let image = image::open(&path)?;
            assert_eq!(
                (image.width(), image.height()),
                export::dimensions(&source, settings)?
            );
            reports.push(serde_json::json!({"file":path.file_name().unwrap().to_string_lossy(),"dimensions":[image.width(),image.height()]}));
        }
    }
    let mut rgba = img.into_raw();
    for (i, p) in rgba.chunks_exact_mut(4).enumerate() {
        p[3] = (i % 256) as u8;
    }
    let original = Source {
        w: 65,
        h: 43,
        rgba: Arc::new(rgba),
        selection: None,
        existing_mask: None,
    };
    for (factor, keep_original) in [(2, true), (4, false)] {
        let workflow = Workflow {
            before: factor,
            keep_original,
            ..Default::default()
        };
        let prepared = cutout::prepare(&original, workflow, &NoProgress)?;
        assert_eq!((prepared.w, prepared.h), (65 * factor, 43 * factor));
        let raw = removal::infer(&prepared, Matte::U2NetP, &NoProgress)?;
        let mask = Arc::new(removal::refine(
            &prepared,
            &raw,
            Default::default(),
            &NoProgress,
        )?);
        let (mut result, mask) = cutout::finish(&original, &prepared, mask, workflow, &NoProgress)?;
        assert_eq!(
            (result.w, result.h),
            if keep_original {
                (65, 43)
            } else {
                (65 * factor, 43 * factor)
            }
        );
        assert_eq!(mask.len(), result.rgba.len());
        if keep_original {
            assert_eq!(result.rgba, original.rgba);
        }
        result.existing_mask = Some(mask);
        for after in [2, 4] {
            let workflow = Workflow {
                cutout_only: true,
                after,
                ..Default::default()
            };
            let (out, mask) = cutout::finish(
                &result,
                &result,
                result.existing_mask.clone().unwrap(),
                workflow,
                &NoProgress,
            )?;
            assert_eq!((out.w, out.h), (result.w * after, result.h * after));
            assert_eq!(mask.len(), out.rgba.len());
            assert!(out.rgba.chunks_exact(4).any(|p| p[3] < 255));
        }
    }
    let cancelled = output.join("cancelled-export.png");
    std::fs::write(&cancelled, b"existing destination")?;
    let progress = super::Report {
        done: Default::default(),
        cancel_at: 1,
    };
    assert!(
        export::save(
            &source,
            export::Settings {
                ai: Some(Default::default()),
                ..Default::default()
            },
            &cancelled,
            &progress
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&cancelled)?, b"existing destination");
    assert!(!std::fs::read_dir(output)?.any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    let report = serde_json::json!({"photo_exports":reports,"developed_crop_rotation_exposure":true,"upscale_first_2_keep_original_and_4_full_size":true,"cutout_2_and_4_mask_alignment_and_alpha":true,"cancel_preserves_existing_destination_and_no_partial":true});
    std::fs::write(
        output.join("pipelines.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
