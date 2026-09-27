use eframe::egui;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Component {
    name: String,
    version: String,
    license: String,
    authors: Vec<String>,
    copyright: Vec<String>,
    repository: Option<String>,
    notices: Vec<usize>,
}
#[derive(Deserialize)]
struct Inventory {
    components: Vec<Component>,
    texts: Vec<String>,
}
fn inventory() -> &'static Inventory {
    static DATA: OnceLock<Inventory> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../vendor/rust-notices/licenses.json"))
            .expect("checked-in license inventory")
    })
}

pub(super) fn show(ui: &mut egui::Ui) {
    ui.strong("Open source credits");
    ui.label("Omadesign includes the following third-party components. Full license and copyright notices are available here offline and in the installed licenses folder.");
    for (name, copyright, license, texts) in [
        (
            "Real-ESRGAN · General x4v3, x4plus, x2plus, anime 6B",
            "Copyright (c) 2021, Xintao Wang",
            "BSD-3-Clause",
            &[include_str!("../../vendor/ml-notices/RealESRGAN-LICENSE")][..],
        ),
        (
            "BasicSR · model architecture and conversion",
            "Copyright 2018–2022 BasicSR Authors",
            "Apache-2.0",
            &[include_str!("../../vendor/ml-notices/BasicSR-LICENSE")][..],
        ),
        (
            "U²-Net / U²-NetP",
            "Xuebin Qin, Zichen Zhang, Chenyang Huang, Masood Dehghan, Osmar R. Zaiane, Martin Jagersand",
            "Apache-2.0",
            &[include_str!("../../vendor/ml-notices/U2Net-LICENSE")][..],
        ),
        (
            "ONNX Runtime 1.28.0",
            "Copyright (c) Microsoft Corporation",
            "MIT",
            &[
                include_str!("../../vendor/ml-notices/ONNXRuntime-LICENSE"),
                include_str!("../../vendor/ml-notices/ONNXRuntime-ThirdPartyNotices.txt"),
            ][..],
        ),
        (
            "ort / ort-sys 2.0.0-rc.13",
            "Copyright (c) 2023–2026 pyke.io; Copyright (c) 2020 Nicolas Bigaouette",
            "MIT OR Apache-2.0",
            &[
                include_str!("../../vendor/ml-notices/ort-LICENSE-MIT"),
                include_str!("../../vendor/ml-notices/ort-LICENSE-APACHE"),
            ][..],
        ),
        (
            "DIS / IS-Net · optional download",
            "Xuebin Qin and DIS contributors",
            "Apache-2.0",
            &[include_str!("../../vendor/ml-notices/ISNet-LICENSE")][..],
        ),
        (
            "LibRaw 0.22.2",
            "Copyright (C) 2008–2025 LibRaw LLC and credited contributors",
            "CDDL-1.0 (selected option)",
            &[
                include_str!("../../vendor/libraw/COPYRIGHT"),
                include_str!("../../vendor/libraw/LICENSE.CDDL"),
            ][..],
        ),
        (
            "MozJPEG / libjpeg-turbo / IJG",
            "Independent JPEG Group; D. R. Commander and credited contributors",
            "IJG AND Zlib AND BSD-3-Clause",
            &[
                include_str!("../../vendor/native-notices/LICENSE-MozJPEG"),
                include_str!("../../vendor/native-notices/LICENSE-libjpeg-turbo"),
                include_str!("../../vendor/native-notices/README-IJG"),
            ][..],
        ),
        (
            "zlib",
            "Copyright (C) 1995–2026 Jean-loup Gailly and Mark Adler",
            "Zlib",
            &[include_str!("../../vendor/native-notices/LICENSE-zlib")][..],
        ),
        (
            "LLVM libc++ / libc++abi / libunwind",
            "LLVM Project and credited contributors",
            "Apache-2.0 WITH LLVM-exception; legacy MIT/UIUC notices",
            &[
                include_str!("../../vendor/native-notices/LICENSE-libcxx"),
                include_str!("../../vendor/native-notices/LICENSE-libcxxabi"),
                include_str!("../../vendor/native-notices/LICENSE-libunwind"),
            ][..],
        ),
        (
            "Lua 5.4.9",
            "Copyright (C) 1994–2026 Lua.org, PUC-Rio",
            "MIT",
            &[include_str!("../../vendor/lua-notices/LICENSE-Lua-5.4")][..],
        ),
        (
            "Phosphor icons",
            "Copyright (c) 2023 Romet Tagobert",
            "MIT",
            &[include_str!("../../assets/phosphor/LICENSE-MIT")][..],
        ),
    ] {
        ui.push_id(name, |ui| {
            ui.collapsing(format!("{name} · {license}"), |ui| {
                ui.label(copyright);
                for (i, text) in texts.iter().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.collapsing("Full license and notices", |ui| {
                            ui.add(egui::Label::new(*text).wrap());
                        });
                    });
                }
            });
        });
    }
    ui.separator();
    let data = inventory();
    ui.strong(format!(
        "Rust dependencies · {} components",
        data.components.len()
    ));
    let key = egui::Id::new("credits-search");
    let mut query = ui
        .ctx()
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_default();
    ui.add(egui::TextEdit::singleline(&mut query).hint_text("Find a component or license"));
    ui.ctx().data_mut(|d| d.insert_temp(key, query.clone()));
    let query = query.to_lowercase();
    for c in &data.components {
        if !query.is_empty()
            && !format!("{} {}", c.name, c.license)
                .to_lowercase()
                .contains(&query)
        {
            continue;
        }
        ui.push_id((&c.name, &c.version), |ui| {
            ui.collapsing(format!("{} {} · {}", c.name, c.version, c.license), |ui| {
                for copyright in &c.copyright {
                    ui.label(copyright);
                }
                if !c.authors.is_empty() {
                    ui.label(format!("Authors: {}", c.authors.join(", ")));
                }
                if let Some(url) = &c.repository {
                    ui.hyperlink_to("Upstream project ↗", url);
                }
                for &index in &c.notices {
                    ui.push_id(index, |ui| {
                        ui.collapsing("Full license and copyright notices", |ui| {
                            ui.add(egui::Label::new(&data.texts[index]).wrap());
                        });
                    });
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn credits_inventory_matches_lockfile_and_retains_full_notices() {
        use sha2::{Digest, Sha256};
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../../vendor/rust-notices/licenses.json")).unwrap();
        assert_eq!(
            value["cargo_lock_sha256"].as_str().unwrap(),
            format!("{:x}", Sha256::digest(include_bytes!("../../Cargo.lock")))
        );
        let data = super::inventory();
        assert!(data.components.len() > 400);
        for c in &data.components {
            assert!(!c.notices.is_empty());
            for &n in &c.notices {
                assert!(!data.texts[n].is_empty());
            }
        }
        assert!(data.components.iter().any(|c| c.name == "ort"));
    }
}
