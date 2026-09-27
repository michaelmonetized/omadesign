//! Editable path brackets and type geometry controls.
use crate::{app::Studio,geom::{Geom,Pt},text_geometry::{self,PathAlign}};
use eframe::egui::{self,Ui,Rect,Stroke,Color32,Sense};
pub fn path_inspector(ui:&mut Ui,studio:&mut Studio) {
    let Some(run)=studio.selected_type() else{return};
    let Some(mut on)=run.on_path.clone() else{return};
    let before=on.clone();
    egui::CollapsingHeader::new("Type on path").default_open(true).show(ui,|ui| {
        let length=on.cache.as_ref().map(|p|p.length).unwrap_or(0.);
        ui.horizontal(|ui| {ui.label("Start %");ui.add(egui::DragValue::new(&mut on.start).speed(0.001).custom_formatter(|v,_|format!("{:.1}",v*100.)).custom_parser(|s|s.parse::<f64>().ok().map(|v|v/100.)));});
        ui.horizontal(|ui| {ui.label("End %");ui.add(egui::DragValue::new(&mut on.end).speed(0.001).custom_formatter(|v,_|format!("{:.1}",v*100.)).custom_parser(|s|s.parse::<f64>().ok().map(|v|v/100.)));});
        if length > 0. {
            let mut start=on.start*length; let mut end=on.end*length;
            ui.horizontal(|ui| {ui.label("Start px"); if ui.add(egui::DragValue::new(&mut start).speed(0.5)).changed(){on.start=start/length;} });
            ui.horizontal(|ui| {ui.label("End px"); if ui.add(egui::DragValue::new(&mut end).speed(0.5)).changed(){on.end=end/length;} });
        }
        ui.checkbox(&mut on.flip,"Flip side and direction");
        ui.horizontal(|ui|{ui.label("Baseline shift");ui.add(egui::DragValue::new(&mut on.baseline_shift).speed(0.25).suffix(" px"));});
        ui.horizontal(|ui|{ui.label("Curve spacing");ui.add(egui::DragValue::new(&mut on.spacing).speed(0.1).suffix(" px"));});
        ui.label("Align to path");
        egui::ComboBox::from_id_salt("path-alignment").selected_text(format!("{:?}",on.align)).show_ui(ui,|ui|{
            for value in [PathAlign::Baseline,PathAlign::Center,PathAlign::Top,PathAlign::Bottom] {ui.selectable_value(&mut on.align,value,format!("{value:?}"));}
        });
    });
    if before!=on {studio.patch_type(|t|{t.on_path=Some(on.clone());t.layout=None;});}
}
pub fn menu(ui:&mut Ui,studio:&mut Studio) {
    if ui.add_enabled(studio.can_attach_text_path(),egui::Button::new("Attach text to path")).clicked(){studio.attach_text_path();ui.close();}
    let attached=studio.selected_type().is_some_and(|t|t.on_path.is_some());
    if ui.add_enabled(attached,egui::Button::new("Release text from path")).clicked(){studio.release_text_path();ui.close();}
}
pub fn brackets(ui:&mut Ui,rect:Rect,studio:&mut Studio)->bool {
    if studio.tool==crate::tools::Tool::Node{return false;}
    let painter=ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground,egui::Id::new("text-geometry-brackets"))).with_clip_rect(rect);
    let mut handled=false;
    let selection=studio.selection.clone();
    for (layer,id) in selection {
        let Some(shape)=studio.doc.find_shape(layer,id) else{continue};
        let Geom::Text(current)=shape.geom.clone() else{continue};
        let Some(on)=current.on_path.as_ref() else{continue};
        let Some(path)=on.cache.as_ref() else{continue};
        let Some((_,span))=text_geometry::path_interval(on) else{continue};
        let view=studio.view;
        for (kind,distance) in [(0,0.),(1,span),(2,span*0.5)] {
            let Some((point,tangent))=text_geometry::path_position(&current,distance) else{continue};
            let to_screen=|p:Pt|{let p=view.to_screen(p);egui::pos2(rect.min.x+p.x,rect.min.y+p.y)};
            let center=to_screen(point)+egui::vec2(tangent.x,tangent.y)*match kind{0=>-6.,1=>6.,_=>0.};
            let color=if kind==1&&current.layout.as_ref().is_some_and(|l|l.overflow){Color32::from_rgb(245,65,75)}else{crate::ui::theme::select()};
            painter.line_segment([to_screen(point-tangent.perp()*9./view.scale),to_screen(point+tangent.perp()*9./view.scale)],Stroke::new(2.,color));
            painter.circle_filled(center,3.,color);
            if kind==1&&current.layout.as_ref().is_some_and(|l|l.overflow){painter.text(center+egui::vec2(9.,-8.),egui::Align2::CENTER_CENTER,"+",egui::FontId::proportional(15.),color);}
            let response=ui.interact(Rect::from_center_size(center,egui::vec2(16.,22.)),egui::Id::new(("type-path-bracket",id,kind)),Sense::drag());
            let drag_id=egui::Id::new(("type-path-before",id));
            if response.drag_started(){ui.ctx().data_mut(|d|d.insert_temp(drag_id,current.clone()));}
            if response.dragged() && let Some(pos)=response.interact_pointer_pos(){
                let world=view.pointer_to_world(Pt::new(rect.min.x,rect.min.y),Pt::new(pos.x,pos.y));
                let fraction=path.nearest_distance(world)/path.length.max(0.001);
                let mut next=on.clone();
                match kind {
                    0=>{let length=on.end-on.start;next.start=fraction;if path.closed{next.end=fraction+length;}},
                    1=>next.end=if path.closed&&fraction<on.start{fraction+1.}else{fraction},
                    _=>{let (p,t)=path.point_and_tangent_at(path.nearest_distance(world));next.flip=(world-p).dot(t.perp())>0.;},
                }
                if let Some(s)=studio.doc.find_shape_mut(layer,id)&&let Geom::Text(t)=&mut s.geom{t.on_path=Some(next);t.layout=None;}
                studio.mark();handled=true;
            }
            if response.drag_stopped()&&let Some(before)=ui.ctx().data_mut(|d|d.remove_temp::<crate::geom::TypeRun>(drag_id))&&let Some(s)=studio.doc.find_shape(layer,id){
                let after=s.geom.clone();let rotation=s.rotation;
                studio.history.push(crate::document::Cmd::SetGeom{layer,id,before:Geom::Text(before),after,rot_before:rotation,rot_after:rotation});studio.dirty=true;handled=true;
            }
            if response.hovered(){handled=true;}
        }
    }
    handled
}
