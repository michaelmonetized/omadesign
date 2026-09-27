use super::*;

impl Studio {
    /// A selected layer is one movable unit; a canvas selection retains group units.
    pub fn align_targets(&self) -> (Vec<(usize, u64)>, bool) {
        if !self.selection.is_empty() {
            return (
                self.selection
                    .iter()
                    .copied()
                    .filter(|&(li, id)| align::eligible_target(&self.doc, li, id, self.individual_object, false))
                    .collect(),
                false,
            );
        }
        let Some(index) = self
            .selected_layer
            .and_then(|id| self.doc.layers.iter().position(|l| l.id == id))
        else {
            return (vec![], false);
        };
        if !self.layer_unlocked(index) || !self.doc.layer_visible(index) {
            return (vec![], true);
        }
        let targets = self
            .layer_tree_indices(index)
            .into_iter()
            .flat_map(|li| {
                if let Some(shapes) = self.doc.layers[li].kind.shapes() {
                    shapes
                        .iter()
                        .filter(|s| align::eligible_target(&self.doc, li, s.id, self.individual_object, true))
                        .map(|s| (li, s.id))
                        .collect::<Vec<_>>()
                } else if align::eligible_target(&self.doc, li, RASTER_ID, self.individual_object, true) {
                    vec![(li, RASTER_ID)]
                } else {
                    vec![]
                }
            })
            .collect();
        (targets, true)
    }

    pub fn alignment_item_count(&self) -> usize {
        let (ids, unit) = self.align_targets();
        align::item_count(&self.doc, &ids, self.individual_object, unit)
    }

    pub fn can_align_selection(&self) -> bool {
        self.alignment_item_count() > 0
    }

    pub fn align_sel(&mut self, how: Align) {
        self.align_sel_to(how, align::AlignTo::Auto);
    }

    pub fn align_sel_to(&mut self, how: Align, reference: align::AlignTo) {
        crate::telemetry::count("feature.align");
        let (ids, unit) = self.align_targets();
        let deltas = align::align_with_reference(
            &self.doc,
            &ids,
            how,
            self.individual_object,
            unit,
            reference,
        );
        self.apply_deltas(&deltas);
        self.status = if unit
            || self.alignment_item_count() == 1
            || reference == align::AlignTo::EachToArtboard
        {
            "Aligned to artboard"
        } else {
            "Aligned selection"
        }
        .into();
    }

    pub fn distribute_sel(&mut self, how: Distribute) {
        let (ids, unit) = self.align_targets();
        if unit {
            return;
        }
        let deltas = align::distribute_items(&self.doc, &ids, how, self.individual_object);
        self.apply_deltas(&deltas);
        self.status = "distributed".into();
    }
}

#[cfg(test)]
#[path = "alignment_tests.rs"]
mod tests;
