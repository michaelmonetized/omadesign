use super::*;
use std::sync::Arc;

impl Studio {
    pub fn apply_project_font(
        &mut self,
        font: &crate::typography::LoadedFont,
    ) -> Result<(), String> {
        crate::text::register_memory_font(&font.id, &font.family, Arc::clone(&font.bytes))?;
        let id = font.id.clone();
        self.patch_type(|run| run.font = id.clone());
        self.text_font = id;
        self.font_scroll_once = true;
        self.status = "Project font applied · stored with this brand".into();
        Ok(())
    }

    pub(crate) fn refresh_project_type(&mut self) {
        let Some(kit) = &self.libraries.typography else {
            return;
        };
        let mut changed = false;
        for layer in &mut self.doc.layers {
            if let Some(shapes) = layer.kind.shapes_mut() {
                for shape in shapes {
                    if let Geom::Text(run) = &mut shape.geom
                        && kit.fonts.iter().any(|font| font.id == run.font)
                    {
                        let contours = crate::text::shape(run);
                        if run.contours != contours {
                            run.contours = contours;
                            changed = true;
                        }
                    }
                }
            }
        }
        if changed {
            self.mark();
        }
    }
}
