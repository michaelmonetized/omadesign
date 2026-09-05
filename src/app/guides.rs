use super::*;
use crate::document::{RulerSettings, RulerUnit};

impl Studio {
    pub fn move_guide(&mut self, index: usize, pos: f32) {
        if !pos.is_finite() || self.doc.guides.get(index).is_none_or(|g| g.pos == pos) {
            return;
        }
        let mut after = self.doc.guides.clone();
        after[index].pos = pos;
        self.commit(Cmd::SetGuides {
            before: self.doc.guides.clone(),
            after,
        });
        self.status = "Guide moved".into();
    }

    pub fn remove_guide(&mut self, index: usize) {
        if let Some(guide) = self.doc.guides.get(index).copied() {
            self.commit(Cmd::RemoveGuide { index, guide });
            self.status = "Guide removed".into();
        }
    }

    pub fn clear_guides(&mut self) {
        if !self.doc.guides.is_empty() {
            self.commit(Cmd::SetGuides {
                before: self.doc.guides.clone(),
                after: vec![],
            });
            self.status = "Guides cleared · undo to bring them back".into();
        }
    }

    fn set_ruler(&mut self, after: RulerSettings) {
        if self.doc.ruler != after {
            self.commit(Cmd::SetRuler {
                before: self.doc.ruler,
                after,
            });
        }
    }

    pub fn toggle_guides(&mut self) {
        self.set_ruler(RulerSettings {
            guides_visible: !self.doc.ruler.guides_visible,
            ..self.doc.ruler
        });
        self.status = if self.doc.ruler.guides_visible {
            "Guides shown"
        } else {
            "Guides hidden"
        }
        .into();
    }

    pub fn set_ruler_origin(&mut self, origin: Pt) {
        if origin.x.is_finite() && origin.y.is_finite() {
            self.set_ruler(RulerSettings {
                origin,
                ..self.doc.ruler
            });
            self.status = "Ruler origin set".into();
        }
    }

    pub fn set_ruler_unit(&mut self, unit: RulerUnit) {
        self.set_ruler(RulerSettings {
            unit,
            ..self.doc.ruler
        });
        self.status = format!("Ruler units · {}", unit.label());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Guide;

    #[test]
    fn guide_edits_restore_exact_order_and_duplicates_through_undo_redo() {
        let mut s = Studio::new();
        s.doc.guides = vec![
            Guide {
                vertical: true,
                pos: 10.0,
            },
            Guide {
                vertical: false,
                pos: 20.0,
            },
            Guide {
                vertical: true,
                pos: 30.0,
            },
        ];
        let original = s.doc.guides.clone();
        s.remove_guide(1);
        s.undo();
        assert_eq!(s.doc.guides, original);
        s.redo();
        assert_eq!(s.doc.guides, vec![original[0], original[2]]);
        s.undo();
        s.add_guide(true, 10.0);
        s.undo();
        assert_eq!(s.doc.guides, original);
        s.move_guide(1, 42.0);
        s.undo();
        assert_eq!(s.doc.guides, original);
        s.redo();
        assert_eq!(s.doc.guides[1].pos, 42.0);
        let moved = s.doc.guides.clone();
        s.clear_guides();
        assert!(s.doc.guides.is_empty());
        s.undo();
        assert_eq!(s.doc.guides, moved);
    }

    #[test]
    fn rulers_roundtrip_and_old_documents_default_to_visible_pixel_guides() {
        let old = r#"{"name":"legacy","width":200,"height":100,"dpi":300,"layers":[],"guides":[{"vertical":true,"pos":20}],"grid":{"visible":false,"snap":true,"size":8,"subdivisions":1}}"#;
        let mut doc: Document = serde_json::from_str(old).unwrap();
        assert_eq!(doc.ruler, RulerSettings::default());
        doc.ruler = RulerSettings {
            origin: Pt::new(15.0, -20.0),
            unit: RulerUnit::Inches,
            guides_visible: false,
        };
        let roundtrip: Document =
            serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(roundtrip.ruler, doc.ruler);
        assert_eq!(roundtrip.guides, doc.guides);
        for (unit, expected) in [
            (RulerUnit::Pixels, 300.0),
            (RulerUnit::Millimeters, 25.4),
            (RulerUnit::Centimeters, 2.54),
            (RulerUnit::Inches, 1.0),
            (RulerUnit::Points, 72.0),
        ] {
            assert!((300.0 / unit.pixels_per_unit(300.0) - expected).abs() < 0.001);
        }
    }
}
