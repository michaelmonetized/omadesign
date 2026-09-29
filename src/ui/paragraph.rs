use crate::{
    app::Studio,
    geom::{BreakMode, KeepTogether, LastLine, TextAlign},
};
use eframe::egui::{self, Ui};

pub fn show(ui: &mut Ui, studio: &mut Studio) {
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
        ui.spacing_mut().item_spacing.x=4.;
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
            let selected=p.align==align;
            let clicked=match align {
                TextAlign::Start=>super::icons::icon_button(ui,"\u{e484}",name,selected),
                TextAlign::Center=>super::icons::icon_button(ui,"\u{e480}",name,selected),
                TextAlign::End=>super::icons::icon_button(ui,"\u{e486}",name,selected),
                TextAlign::Justify{last}=>{
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(30.,28.),egui::Sense::click());
                    response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Button,ui.is_enabled(),selected,name));
                    if selected||response.hovered(){ui.painter().rect_filled(rect.shrink(1.),6.,if selected{super::theme::accent_soft()}else{super::theme::bg_widget_hover()});}
                    let color=if selected{super::theme::accent()}else{super::theme::fg_weak()};
                    for row in 0..4 {let width=if row==3&&last!=LastLine::Justify{10.}else{18.};let shift=if row==3{match last{LastLine::Center=>4.,LastLine::End=>8.,_=>0.}}else{0.};let x=rect.min.x+6.+shift;let y=rect.min.y+6.+row as f32*5.;ui.painter().line_segment([egui::pos2(x,y),egui::pos2(x+width,y)],egui::Stroke::new(1.2,color));}
                    response.on_hover_text(match last{LastLine::Start=>"Justify, last line left",LastLine::Center=>"Justify, last line centered",LastLine::End=>"Justify, last line right",LastLine::Justify=>"Justify all lines"}).clicked()
                }
            };
            if clicked{p.align=align;}

        }
    });
    let mut wrap = run.wrap_width.is_some();
    let mut width = run.wrap_width.unwrap_or(320.);
    let mut changed = ui.checkbox(&mut wrap, "Wrap text to width").changed();
    if wrap {
        changed |= ui
            .add(
                egui::DragValue::new(&mut width)
                    .range(1.0..=10000.0)
                    .prefix("Width ")
                    .suffix(" px"),
            )
            .changed();
    }
    if changed {
        studio.patch_type(|t| t.wrap_width = wrap.then_some(width));
    }
    ui.collapsing("Justification", |ui| {
        egui::Grid::new("justification_limits").show(ui, |ui| {
            ui.label("");
            ui.label("Min");
            ui.label("Desired");
            ui.label("Max");
            ui.end_row();
            for (label, values, range) in [
                ("Word %", &mut p.word_spacing, 1.0..=1000.0),
                ("Letter %", &mut p.letter_spacing, -100.0..=1000.0),
            ] {
                ui.label(label);
                for v in values.iter_mut() {
                    ui.add(egui::DragValue::new(v).range(range.clone()).speed(1.));
                }
                ui.end_row();
                values.sort_by(f32::total_cmp);
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
                ui.label("Language");
                ui.text_edit_singleline(&mut h.language);
            });
            for (label, v) in [
                ("Minimum word length", &mut h.min_word_len),
                ("Before hyphen", &mut h.min_before),
                ("After hyphen", &mut h.min_after),
                ("Consecutive lines", &mut h.max_consecutive),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    let minimum = usize::from(label != "Consecutive lines");
                    ui.add(egui::DragValue::new(v).range(minimum..=30))
                        .on_hover_text(if minimum == 0 { "0 allows unlimited consecutive hyphenated lines" } else { label });
                });
            }
            ui.add(
                egui::DragValue::new(&mut h.zone)
                    .range(0.0..=1000.0)
                    .prefix("Zone ")
                    .suffix(" px"),
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
    {
        studio.patch_character(|s| s.no_break = no_break);
    }
    ui.collapsing("Keeps & widows", |ui| {
        ui.checkbox(&mut p.allow_orphans, "Allow orphans");
        ui.add(
            egui::DragValue::new(&mut p.min_start_lines)
                .range(1..=20)
                .prefix("Orphan minimum "),
        );
        ui.add(
            egui::DragValue::new(&mut p.min_end_lines)
                .range(1..=20)
                .prefix("Widow minimum "),
        );
        let mut runt = p.runt.is_some();
        if ui.checkbox(&mut runt, "Avoid short last line").changed() {
            p.runt = runt.then(Default::default);
        }
        if let Some(rule) = &mut p.runt {
            ui.add(
                egui::DragValue::new(&mut rule.words)
                    .range(1..=10)
                    .prefix("Minimum words "),
            );
            ui.add(
                egui::DragValue::new(&mut rule.characters)
                    .range(1..=50)
                    .prefix("Minimum characters "),
            );
        }
        ui.add(
            egui::DragValue::new(&mut p.keep_with_next)
                .range(0..=20)
                .prefix("Keep with next lines "),
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
            ui.add(egui::DragValue::new(first).range(1..=20).prefix("First "));
            ui.add(egui::DragValue::new(last).range(1..=20).prefix("Last "));
        }
    });
    if p != before {
        studio.patch_paragraph(|target|{
        macro_rules! changed {($($field:ident),*)=>{$(if p.$field!=before.$field{target.$field=p.$field.clone();})*};}
        changed!(align,word_spacing,letter_spacing,break_mode,overflow_wrap,hyphenate,hyphen,min_start_lines,min_end_lines,allow_orphans,runt,keep_with_next,keep_together);
    });
    }
}
