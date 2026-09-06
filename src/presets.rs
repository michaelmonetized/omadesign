//! New-document sizes a designer would actually reach for.

#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub group: &'static str,
    pub name: &'static str,
    pub w: f32,
    pub h: f32,
    pub dpi: f32,
}

impl Preset {
    pub fn pixels(self) -> (u32, u32) {
        (self.w.round() as u32, self.h.round() as u32)
    }
}

pub fn all() -> &'static [Preset] {
    &[
        Preset {
            group: "Print",
            name: "A4",
            w: 2480.0,
            h: 3508.0,
            dpi: 300.0,
        },
        Preset {
            group: "Print",
            name: "A4 landscape",
            w: 3508.0,
            h: 2480.0,
            dpi: 300.0,
        },
        Preset {
            group: "Print",
            name: "Letter",
            w: 2550.0,
            h: 3300.0,
            dpi: 300.0,
        },
        Preset {
            group: "Print",
            name: "A3",
            w: 3508.0,
            h: 4961.0,
            dpi: 300.0,
        },
        Preset {
            group: "Print",
            name: "Business card",
            w: 1050.0,
            h: 600.0,
            dpi: 300.0,
        },
        Preset {
            group: "Screen",
            name: "HD 1920×1080",
            w: 1920.0,
            h: 1080.0,
            dpi: 72.0,
        },
        Preset {
            group: "Screen",
            name: "1280×800",
            w: 1280.0,
            h: 800.0,
            dpi: 72.0,
        },
        Preset {
            group: "Screen",
            name: "4K",
            w: 3840.0,
            h: 2160.0,
            dpi: 72.0,
        },
        Preset {
            group: "Screen",
            name: "Square 1080",
            w: 1080.0,
            h: 1080.0,
            dpi: 72.0,
        },
        Preset {
            group: "Social",
            name: "Instagram post",
            w: 1080.0,
            h: 1080.0,
            dpi: 72.0,
        },
        Preset {
            group: "Social",
            name: "Instagram portrait",
            w: 1080.0,
            h: 1350.0,
            dpi: 72.0,
        },
        Preset {
            group: "Social",
            name: "Story / Reel",
            w: 1080.0,
            h: 1920.0,
            dpi: 72.0,
        },
        Preset {
            group: "Social",
            name: "X / Twitter",
            w: 1600.0,
            h: 900.0,
            dpi: 72.0,
        },
        Preset {
            group: "Identity",
            name: "Logo 2000",
            w: 2000.0,
            h: 2000.0,
            dpi: 72.0,
        },
        Preset {
            group: "Identity",
            name: "App icon 1024",
            w: 1024.0,
            h: 1024.0,
            dpi: 72.0,
        },
        Preset {
            group: "Identity",
            name: "Favicon 512",
            w: 512.0,
            h: 512.0,
            dpi: 72.0,
        },
        Preset {
            group: "Photo",
            name: "3:2 landscape",
            w: 6000.0,
            h: 4000.0,
            dpi: 300.0,
        },
        Preset {
            group: "Photo",
            name: "3:2 portrait",
            w: 4000.0,
            h: 6000.0,
            dpi: 300.0,
        },
        Preset {
            group: "Photo",
            name: "4:3",
            w: 4000.0,
            h: 3000.0,
            dpi: 300.0,
        },
        Preset {
            group: "Photo",
            name: "Square 4000",
            w: 4000.0,
            h: 4000.0,
            dpi: 72.0,
        },
    ]
}

pub fn groups() -> [&'static str; 5] {
    ["Print", "Screen", "Social", "Identity", "Photo"]
}
