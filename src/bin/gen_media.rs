use omadesign::app::Studio;
use omadesign::color::Rgba;
use omadesign::compositor;
use omadesign::document::{Fill, Shape, Style};
use omadesign::geom::{Geom, Pt, TypeRun};
use omadesign::shape_browser;

fn save_jpeg(doc: &omadesign::document::Document, path: &str, quality: u8) {
    let png_bytes = compositor::export_png(doc, 1).expect("png");
    let img = image::load_from_memory(&png_bytes).expect("decode png");
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    enc.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )
    .expect("jpeg encode");
    std::fs::write(path, buf).expect("write");
    println!("wrote {} ({}x{})", path, rgb.width(), rgb.height());
}

fn main() {
    let out_dir = "site/public/media";
    std::fs::create_dir_all(out_dir).unwrap();

    // 1. Design – marks, type, boolean, compound
    {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.seed_demo();
        // Add a compound shape to showcase compound tool
        let rect1 = Shape::new(
            Geom::Rect {
                origin: Pt::new(900.0, 200.0),
                size: Pt::new(200.0, 200.0),
                radius: 12.0,
            },
            Style {
                fill: Fill::Solid(Rgba::from_hex(0x2EC4B6)),
                stroke: None,
            },
        );
        let rect2 = Shape::new(
            Geom::Rect {
                origin: Pt::new(1000.0, 300.0),
                size: Pt::new(200.0, 200.0),
                radius: 12.0,
            },
            Style {
                fill: Fill::Solid(Rgba::from_hex(0xFFD166)),
                stroke: None,
            },
        );
        // Commit them then combine to show compound
        s.commit(omadesign::document::Cmd::AddShape {
            layer: 1,
            shape: rect1,
        });
        s.commit(omadesign::document::Cmd::AddShape {
            layer: 1,
            shape: rect2,
        });
        // Save
        save_jpeg(&s.doc, &format!("{out_dir}/design.jpg"), 90);
    }

    // 2. Paint – brush on pixel layer
    {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = omadesign::document::Document::new("paint", 1600.0, 900.0, 72.0);
        s.active_layer = Some(0);
        if let Some(px) = s.doc.layers[0].kind.pixels_mut()
            && let Some(mut pm) = px.to_pixmap()
        {
            let brush = omadesign::paint::Brush {
                size: 64.0,
                hardness: 0.3,
                opacity: 0.9,
                flow: 0.9,
                spacing: 0.15,
                color: Rgba::from_hex(0xE5484D),
            };
            // Draw a few strokes
            omadesign::paint::stroke_to(
                &mut pm,
                Pt::new(200.0, 300.0),
                Pt::new(1400.0, 320.0),
                &brush,
                false,
            );
            omadesign::paint::stroke_to(
                &mut pm,
                Pt::new(300.0, 500.0),
                Pt::new(1300.0, 520.0),
                &brush,
                false,
            );
            omadesign::paint::stroke_to(
                &mut pm,
                Pt::new(400.0, 700.0),
                Pt::new(1200.0, 720.0),
                &brush,
                false,
            );
            *px = omadesign::document::Pixels::from_pixmap(&pm);
        }
        // Add some vector shapes on top
        let star = Shape::new(
            Geom::Star {
                center: Pt::new(800.0, 450.0),
                outer: Pt::splat(120.0),
                inner: 0.4,
                points: 5,
            },
            Style {
                fill: Fill::Solid(Rgba::from_hex(0x4F8CFF)),
                stroke: None,
            },
        );
        s.commit(omadesign::document::Cmd::AddShape {
            layer: 1,
            shape: star,
        });
        save_jpeg(&s.doc, &format!("{out_dir}/paint.jpg"), 90);
    }

    // 3. Photo – develop, then Place in Design (use sample photo)
    {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.photo.import_samples();
        s.send_photo_to_design();
        // Add a text overlay to show photo+design
        s.place_text(Pt::new(80.0, 80.0));
        if let Some(edit) = s.type_edit.take() {
            // Commit the text
            s.type_edit = Some(edit);
            s.commit_type_edit();
        }
        save_jpeg(&s.doc, &format!("{out_dir}/photo.jpg"), 90);
    }

    // 4. Type – caret, Character, OpenType, Google Fonts
    {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = omadesign::document::Document::new("type", 1600.0, 800.0, 72.0);
        let run = TypeRun {
            origin: Pt::new(120.0, 240.0),
            content: "Inter  ·  Google Fonts  ·  palettes".into(),
            px: 72.0,
            tracking: -0.5,
            leading: 0.0,
            font: s.text_font.clone(),
            kern: true,
            liga: true,
            tnum: false,
            smcp: false,
            contours: vec![],
        };
        omadesign::text::fill_contours(&mut Geom::Text(run.clone()));
        let mut g = Geom::Text(run);
        omadesign::text::fill_contours(&mut g);
        let shape = Shape::new(
            g,
            Style {
                fill: Fill::Solid(Rgba::from_hex(0x073B4C)),
                stroke: None,
            },
        );
        s.commit(omadesign::document::Cmd::AddShape { layer: 1, shape });

        let run2 = TypeRun {
            origin: Pt::new(120.0, 420.0),
            content: " Compound  →  Union  Subtract  Intersect ".into(),
            px: 42.0,
            tracking: 0.5,
            ..TypeRun::default()
        };
        let mut g2 = Geom::Text(run2);
        omadesign::text::fill_contours(&mut g2);
        let shape2 = Shape::new(
            g2,
            Style {
                fill: Fill::Solid(Rgba::from_hex(0xE5484D)),
                stroke: None,
            },
        );
        s.commit(omadesign::document::Cmd::AddShape {
            layer: 1,
            shape: shape2,
        });

        // Add a palette swatch strip as shapes
        let swatches = [0xFFD166u32, 0x06D6A0, 0x118AB2, 0xEF476F, 0xFFC6FF];
        for (i, &hex) in swatches.iter().enumerate() {
            let sq = Shape::new(
                Geom::Rect {
                    origin: Pt::new(120.0 + i as f32 * 110.0, 560.0),
                    size: Pt::new(100.0, 100.0),
                    radius: 12.0,
                },
                Style {
                    fill: Fill::Solid(Rgba::from_hex(hex)),
                    stroke: None,
                },
            );
            s.commit(omadesign::document::Cmd::AddShape {
                layer: 1,
                shape: sq,
            });
        }
        save_jpeg(&s.doc, &format!("{out_dir}/type.jpg"), 90);
    }

    // 5. Shapes – icon browser (Phosphor etc.)
    {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = omadesign::document::Document::new("shapes", 1600.0, 900.0, 72.0);
        // Place a few icons as polys
        let icons = ["house", "heart", "star", "gear", "camera", "globe"];
        for (i, name) in icons.iter().enumerate() {
            let icon = shape_browser::Icon {
                name,
                lib: "Phosphor",
            };
            if let Ok(svg) = shape_browser::fetch_svg(&icon) {
                if let Ok(mut geom) = shape_browser::svg_to_geom(&svg, 180.0) {
                    let col = (i as f32 * 60.0) as u32;
                    // Position in grid
                    let x = 120.0 + (i % 3) as f32 * 500.0;
                    let y = 120.0 + (i / 3) as f32 * 360.0;
                    // geom is 0..180, translate to x,y
                    let bbox = geom.bbox();
                    let off = Pt::new(x, y) - bbox.min;
                    geom.translate(off);
                    let shape = Shape::new(
                        geom,
                        Style {
                            fill: Fill::Solid(Rgba::from_hex(0x073B4C + col * 100)),
                            stroke: None,
                        },
                    );
                    s.commit(omadesign::document::Cmd::AddShape { layer: 1, shape });
                }
            } else {
                // Fallback star
                let star = Shape::new(
                    Geom::Star {
                        center: Pt::new(200.0 + i as f32 * 250.0, 300.0),
                        outer: Pt::splat(80.0),
                        inner: 0.4,
                        points: 5,
                    },
                    Style {
                        fill: Fill::Solid(Rgba::from_hex(0x4F8CFF)),
                        stroke: None,
                    },
                );
                s.commit(omadesign::document::Cmd::AddShape {
                    layer: 1,
                    shape: star,
                });
            }
        }
        save_jpeg(&s.doc, &format!("{out_dir}/shapes.jpg"), 90);
    }

    // 6. Assets – free asset browser (picsum fallback)
    {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = omadesign::document::Document::new("assets", 1600.0, 900.0, 72.0);
        // Simulate asset grid: use solid color rects as placeholders for assets
        let providers = ["Pixabay", "Pexels", "Picsum"];
        for (i, prov) in providers.iter().enumerate() {
            let r = Shape::new(
                Geom::Rect {
                    origin: Pt::new(80.0 + i as f32 * 500.0, 200.0),
                    size: Pt::new(420.0, 280.0),
                    radius: 16.0,
                },
                Style {
                    fill: Fill::Solid(Rgba::from_hex(match i {
                        0 => 0xFF6B6B,
                        1 => 0x4ECDC4,
                        _ => 0x45B7D1,
                    })),
                    stroke: None,
                },
            );
            s.commit(omadesign::document::Cmd::AddShape { layer: 1, shape: r });
            let label = TypeRun {
                origin: Pt::new(120.0 + i as f32 * 500.0, 380.0),
                content: prov.to_string(),
                px: 48.0,
                ..TypeRun::default()
            };
            let mut g = Geom::Text(label);
            omadesign::text::fill_contours(&mut g);
            let tshape = Shape::new(
                g,
                Style {
                    fill: Fill::Solid(Rgba::WHITE),
                    stroke: None,
                },
            );
            s.commit(omadesign::document::Cmd::AddShape {
                layer: 1,
                shape: tshape,
            });
        }
        save_jpeg(&s.doc, &format!("{out_dir}/assets.jpg"), 90);
    }

    // Also regenerate hero.mp4 via ffmpeg slideshow of the above
    println!("media generated");
}
