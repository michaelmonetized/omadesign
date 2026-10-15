use crate::{
    app::Studio,
    geom::{BreakMode, KeepTogether, LastLine, TextAlign},
};
use eframe::egui::{self, Ui};

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let owner = egui::Id::new(("paragraph-fields", &studio.swap_id, &studio.selection));
    let allowed = super::text_geometry::edits_allowed(studio);
    ui.push_id(owner, |ui| {
        ui.add_enabled_ui(allowed, |ui| contents(ui, studio))
    });
}

fn contents(ui: &mut Ui, studio: &mut Studio) {
    let Some(run) = studio.selected_type() else {
        return;
    };
    let caret = studio
        .type_edit
        .as_ref()
        .map_or(0, |e| e.caret.min(e.anchor));
    let mut p = crate::text::paragraph_style(&run, caret);
    let before = p.clone();
    ui.strong("Paragraph");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.;
        for (name, align) in [
            ("Left", TextAlign::Start),
            ("Center", TextAlign::Center),
            ("Right", TextAlign::End),
            (
                "Justify L",
                TextAlign::Justify {
                    last: LastLine::Start,
                },
            ),
            (
                "Justify C",
                TextAlign::Justify {
                    last: LastLine::Center,
                },
            ),
            (
                "Justify R",
                TextAlign::Justify {
                    last: LastLine::End,
                },
            ),
            (
                "Justify all",
                TextAlign::Justify {
                    last: LastLine::Justify,
                },
            ),
        ] {
            let selected = p.align == align;
            let clicked = match align {
                TextAlign::Start => super::icons::icon_button(ui, "\u{e484}", name, selected),
                TextAlign::Center => super::icons::icon_button(ui, "\u{e480}", name, selected),
                TextAlign::End => super::icons::icon_button(ui, "\u{e486}", name, selected),
                TextAlign::Justify { last } => {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(30., 28.), egui::Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            selected,
                            name,
                        )
                    });
                    if selected || response.hovered() || response.has_focus() {
                        ui.painter().rect_filled(
                            rect.shrink(1.),
                            6.,
                            if selected {
                                super::theme::accent_soft()
                            } else {
                                super::theme::bg_widget_hover()
                            },
                        );
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            rect.expand(2.),
                            3.,
                            ui.visuals().selection.stroke,
                            egui::StrokeKind::Outside,
                        );
                    }
                    let color = if selected {
                        super::theme::accent()
                    } else {
                        super::theme::fg_weak()
                    };
                    for row in 0..4 {
                        let width = if row == 3 && last != LastLine::Justify {
                            10.
                        } else {
                            18.
                        };
                        let shift = if row == 3 {
                            match last {
                                LastLine::Center => 4.,
                                LastLine::End => 8.,
                                _ => 0.,
                            }
                        } else {
                            0.
                        };
                        let x = rect.min.x + 6. + shift;
                        let y = rect.min.y + 6. + row as f32 * 5.;
                        ui.painter().line_segment(
                            [egui::pos2(x, y), egui::pos2(x + width, y)],
                            egui::Stroke::new(1.2, color),
                        );
                    }
                    response
                        .on_hover_text(match last {
                            LastLine::Start => "Justify, last line left",
                            LastLine::Center => "Justify, last line centered",
                            LastLine::End => "Justify, last line right",
                            LastLine::Justify => "Justify all lines",
                        })
                        .clicked()
                }
            };
            if clicked {
                p.align = align;
            }
        }
    });
    if run.frame.is_none() && run.on_path.is_none() {
        let mut wrap = run.wrap_width.is_some();
        let mut width = run.wrap_width.unwrap_or(320.);
        let mut changed = ui.checkbox(&mut wrap, "Wrap text to width").changed();
        if wrap {
            changed |= super::fields::number(
                ui,
                "wrap-width",
                "Text wrap width (px)",
                &mut width,
                1.0..=10000.0,
                " px",
                None,
                Default::default(),
            )
            .changed;
        }
        if ui.is_enabled() && changed {
            studio.patch_type(|t| t.wrap_width = wrap.then_some(width));
        }
    }
    ui.collapsing("Justification", |ui| {
        egui::Grid::new("justification_limits").show(ui, |ui| {
            for (label, values, range) in [
                ("Word spacing", &mut p.word_spacing, 1.0..=1000.0),
                ("Letter spacing", &mut p.letter_spacing, -100.0..=1000.0),
            ] {
                let mut accepted = false;
                for (index, v) in values.iter_mut().enumerate() {
                    let purpose =
                        format!("{label} {} (%)", ["minimum", "desired", "maximum"][index]);
                    accepted |= super::fields::number(
                        ui,
                        (label, index),
                        &purpose,
                        v,
                        range.clone(),
                        " %",
                        None,
                        Default::default(),
                    )
                    .changed;
                }
                ui.end_row();
                if accepted {
                    values.sort_by(f32::total_cmp);
                }
            }
        });
    });
    ui.collapsing("Line breaking", |ui| {
        egui::ComboBox::from_label("Break mode")
            .selected_text(match p.break_mode {
                BreakMode::Normal => "Normal",
                BreakMode::Word => "Word",
                BreakMode::BreakAll => "Break all",
                BreakMode::KeepAll => "No wrap",
            })
            .show_ui(ui, |ui| {
                for (name, mode) in [
                    ("Normal", BreakMode::Normal),
                    ("Word", BreakMode::Word),
                    ("Break all", BreakMode::BreakAll),
                    ("No wrap", BreakMode::KeepAll),
                ] {
                    ui.selectable_value(&mut p.break_mode, mode, name);
                }
            });
        ui.checkbox(&mut p.overflow_wrap, "Allow breaking long words");
        ui.checkbox(&mut p.hyphenate, "Hyphenate");
        ui.collapsing("Hyphenation settings", |ui| {
            let h = p.hyphen.get_or_insert_with(Default::default);
            ui.horizontal(|ui| {
                let label = ui.label("Language");
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut h.language)
                            .id(ui.make_persistent_id("hyphenation-language")),
                    )
                    .labelled_by(label.id);
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Hyphenation language")
                });
            });
            for (label, v) in [
                ("Minimum word length", &mut h.min_word_len),
                ("Before hyphen", &mut h.min_before),
                ("After hyphen", &mut h.min_after),
                ("Consecutive lines", &mut h.max_consecutive),
            ] {
                let minimum = usize::from(label != "Consecutive lines");
                super::fields::number(
                    ui,
                    label,
                    label,
                    v,
                    minimum..=30,
                    "",
                    None,
                    Default::default(),
                )
                .response
                .on_hover_text(if minimum == 0 {
                    "0 allows unlimited consecutive hyphenated lines"
                } else {
                    label
                });
            }
            super::fields::number(
                ui,
                "hyphenation-zone",
                "Hyphenation zone (px)",
                &mut h.zone,
                0.0..=1000.0,
                " px",
                None,
                Default::default(),
            );
            ui.checkbox(&mut h.capitalized, "Hyphenate capitalized words");
            ui.checkbox(&mut h.last_word, "Hyphenate last word");
        });
    });
    let range = studio
        .type_edit
        .as_ref()
        .map(|e| (e.caret.min(e.anchor), e.caret.max(e.anchor)))
        .unwrap_or((0, run.content.chars().count()));
    let mut no_break = run.character_style(range.0).no_break;
    if ui
        .checkbox(&mut no_break, "No break in selected text")
        .changed()
        && ui.is_enabled()
    {
        studio.patch_character(|s| s.no_break = no_break);
    }
    ui.collapsing("Keeps & widows", |ui| {
        ui.checkbox(&mut p.allow_orphans, "Allow orphans");
        super::fields::number(
            ui,
            "orphan-minimum",
            "Orphan minimum (lines)",
            &mut p.min_start_lines,
            1..=20,
            "",
            None,
            Default::default(),
        );
        super::fields::number(
            ui,
            "widow-minimum",
            "Widow minimum (lines)",
            &mut p.min_end_lines,
            1..=20,
            "",
            None,
            Default::default(),
        );
        let mut runt = p.runt.is_some();
        if ui.checkbox(&mut runt, "Avoid short last line").changed() {
            p.runt = runt.then(Default::default);
        }
        if let Some(rule) = &mut p.runt {
            super::fields::number(
                ui,
                "runt-words",
                "Minimum words on last line",
                &mut rule.words,
                1..=10,
                "",
                None,
                Default::default(),
            );
            super::fields::number(
                ui,
                "runt-characters",
                "Minimum characters on last line",
                &mut rule.characters,
                1..=50,
                "",
                None,
                Default::default(),
            );
        }
        super::fields::number(
            ui,
            "keep-with-next",
            "Keep with next (lines)",
            &mut p.keep_with_next,
            0..=20,
            "",
            None,
            Default::default(),
        );
        egui::ComboBox::from_label("Keep lines")
            .selected_text(match p.keep_together {
                KeepTogether::None => "None",
                KeepTogether::All => "All",
                KeepTogether::Edges { .. } => "First / last",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut p.keep_together, KeepTogether::None, "None");
                ui.selectable_value(&mut p.keep_together, KeepTogether::All, "All");
                ui.selectable_value(
                    &mut p.keep_together,
                    KeepTogether::Edges { first: 2, last: 2 },
                    "First / last",
                );
            });
        if let KeepTogether::Edges { first, last } = &mut p.keep_together {
            super::fields::number(
                ui,
                "keep-first",
                "Keep first (lines)",
                first,
                1..=20,
                "",
                None,
                Default::default(),
            );
            super::fields::number(
                ui,
                "keep-last",
                "Keep last (lines)",
                last,
                1..=20,
                "",
                None,
                Default::default(),
            );
        }
    });
    if ui.is_enabled() && p != before {
        studio.patch_paragraph(|target|{
        macro_rules! changed {($($field:ident),*)=>{$(if p.$field!=before.$field{target.$field=p.$field.clone();})*};}
        changed!(align,word_spacing,letter_spacing,break_mode,overflow_wrap,hyphenate,hyphen,min_start_lines,min_end_lines,allow_orphans,runt,keep_with_next,keep_together);
    });
    }
}
