use crate::{
    app::{CloudModal, Studio},
    cloud::{self, client::Event},
};
use eframe::egui::{self, Ui};
use serde_json::json;

#[cfg(test)]
#[path = "cloud_keyboard_tests.rs"]
mod keyboard_tests;

#[derive(Clone)]
struct Owner {
    id: egui::Id,
    document: String,
    modal: std::mem::Discriminant<CloudModal>,
    project: String,
    focus: bool,
}

/// Capture one native cloud dialog draft without changing its modal identity.
/// Args: context and studio. Returns the stable document/project/open-instance scope.
fn owner(ctx: &egui::Context, studio: &Studio) -> Owner {
    ctx.data(|data|data.get_temp::<Owner>(egui::Id::new("cloud-native-owner")))
        .filter(|owner|owner.document==studio.swap_id&&owner.modal==std::mem::discriminant(&studio.cloud_modal)&&owner.project==studio.cloud_panel.project_id)
        .unwrap_or_else(||Owner{id:super::keyboard_controls::draft_id(ctx,"cloud-native-owner"),document:studio.swap_id.clone(),
            modal:std::mem::discriminant(&studio.cloud_modal),project:studio.cloud_panel.project_id.clone(),focus:true})
}

/// Focus an actual enabled dialog field after its modal layer becomes available.
/// Args: UI, open owner and native response. Returns no value.
fn initial_focus(ui: &Ui, owner: &mut Owner, response: &egui::Response) {
    if owner.focus&&!ui.is_sizing_pass()&&response.enabled()&&ui.memory(|memory|memory.allows_interaction(response.layer_id)){
        response.request_focus();owner.focus=false;
    }
}

/// Match the cloud service's trimmed UTF-16 text limits.
/// Args: draft and server limit. Returns whether the current native draft can be submitted.
fn valid_text(value: &str, max: usize) -> bool {
    let value=value.trim();!value.is_empty()&&value.encode_utf16().count()<=max
}

/// Match the existing invitation endpoint's public email grammar.
/// Args: email draft. Returns whether its trimmed text satisfies the service's 254-character limit.
fn valid_email(value: &str) -> bool {
    let value=value.trim();
    valid_text(value,254)&&!value.chars().any(char::is_whitespace)
        && value.split_once('@').is_some_and(|(local,domain)|!local.is_empty()&&!domain.contains('@')
            && domain.rsplit_once('.').is_some_and(|(left,right)|!left.is_empty()&&!right.is_empty()))
}

/// Draw an existing cloud action in its source identity scope.
/// Args: UI, property identity, visible text, native purpose and availability. Returns the real button response.
fn action(ui: &mut Ui, id: egui::Id, visible: &str, purpose: &str, enabled: bool) -> egui::Response {
    super::agent::action(ui,id,visible,purpose,enabled)
}

fn value<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn modal(ui: &mut Ui, studio: &mut Studio) {
    if studio.cloud_modal == CloudModal::None {
        ui.ctx().data_mut(|data|data.remove::<Owner>(egui::Id::new("cloud-native-owner")));
        return;
    }
    let ctx = ui.ctx().clone();
    let mut owner=owner(&ctx,studio);
    let title = match studio.cloud_modal {
        CloudModal::SignIn if cloud::connected(&studio.cloud_identity) => "You're in.",
        CloudModal::SignIn => "Connect to cloud",
        CloudModal::Projects => "Cloud projects",
        CloudModal::Review => "Review & annotations",
        CloudModal::Invite => "Invite a reviewer",
        CloudModal::Publish => "Publish finished work",
        CloudModal::Live => "Live collaboration",
        CloudModal::None => return,
    };
    let dialog=egui::Modal::new(egui::Id::new("cloud-modal")).show(&ctx,|ui|{
 ui.set_width(if matches!(studio.cloud_modal,CloudModal::Review|CloudModal::Publish){760.}else{440.});ui.heading(title);ui.add_space(10.);
 let busy=studio.cloud_busy();
 match studio.cloud_modal {
  CloudModal::Live => live_ui(ui, studio, busy),
  CloudModal::SignIn=>{
   if cloud::connected(&studio.cloud_identity){
    let who = cloud::account_label(&studio.cloud_identity);
    ui.label(format!("This desktop is connected as {who}."));
    ui.label("Projects stay on this machine until you push them.");
    let response=action(ui,owner.id.with("disconnect"),"Disconnect this desktop","Disconnect this desktop from cloud",true);initial_focus(ui,&mut owner,&response);if response.clicked(){studio.disconnect_cloud();}
   }
   else{ui.label("Sign in securely in your browser, then approve this desktop. Your project files stay local until you push them.");if !studio.cloud_panel.code.is_empty(){ui.heading(&studio.cloud_panel.code);ui.label("Check that this code matches the code in your browser.");}let response=action(ui,owner.id.with("sign-in"),"Open browser to sign in","Open browser to sign in to cloud",!busy);initial_focus(ui,&mut owner,&response);if response.clicked(){studio.connect_cloud();}}
  },
  CloudModal::Projects=>{
   let response=action(ui,owner.id.with("refresh-projects"),"Refresh projects","Refresh cloud projects",!busy);initial_focus(ui,&mut owner,&response);if response.clicked(){studio.show_cloud_projects();}
   let projects=studio.cloud_panel.projects.clone();egui::ScrollArea::vertical().max_height(480.).show(ui,|ui|{for project in projects{ui.horizontal(|ui|{ui.label(format!("{} · {}",project.title,project.role));if action(ui,owner.id.with(("pull-project",&project.id)),"Pull & open",&format!("Pull and open cloud project {}",project.title),!busy&&project.role!="reviewer").clicked(){studio.pull_cloud_project(project.id);}});}});
   ui.label("Pull opens a separate document and preserves your current work.");
  },
  CloudModal::Invite=>{ui.label("Reviewers can see flat exports and comments. Manage editor access from the cloud project on the website.");let email=super::agent::text(ui,owner.id.with("reviewer-email"),"Reviewer email",&mut studio.invite_email,false);initial_focus(ui,&mut owner,&email);let valid=valid_email(&studio.invite_email);if !valid{super::welcome::control_error(ui,&[&email],"Enter a valid reviewer email (up to 254 characters)");}if action(ui,owner.id.with("invite"),"Send review invitation","Send review invitation",!busy&&valid).clicked(){studio.invite_collaborator();}},
  CloudModal::Review|CloudModal::Publish=>{
   ui.horizontal(|ui|{let response=action(ui,owner.id.with("refresh-review"),"Refresh cloud review","Refresh cloud review",!busy);initial_focus(ui,&mut owner,&response);if response.clicked(){studio.refresh_cloud_review();}if action(ui,owner.id.with("open-web"),"Open web project ↗","Open cloud project on the web",!studio.cloud_panel.project_id.is_empty()).clicked(){if !studio.cloud_panel.project_id.is_empty(){let _=std::process::Command::new("xdg-open").arg(format!("https://omadesign.app/cloud?project={}",studio.cloud_panel.project_id)).spawn();}}});
   let snapshots:Vec<_>=studio.cloud_panel.files.iter().filter(|f|f.kind=="snapshot").cloned().collect();
   let choices=snapshots.iter().map(|file|(file.id.clone(),format!("{} · v{}",file.name,file.version))).collect::<Vec<_>>();
   let snapshot=super::fields::native_scope(ui,owner.id.with("snapshot"),|ui|super::keyboard_controls::choice(ui,"value","Cloud flat export",&mut studio.cloud_panel.selected_snapshot,&choices)).inner;
   let snapshot_valid=snapshots.iter().any(|file|file.id==studio.cloud_panel.selected_snapshot);
   if !studio.cloud_panel.selected_snapshot.is_empty()&&!snapshot_valid{super::welcome::control_error(ui,&[&snapshot],"Select an available flat export");}
   if action(ui,owner.id.with("view-export"),"View selected export","View selected cloud export",!busy&&snapshot_valid).clicked(){studio.load_cloud_preview();}
   let annotations=studio.cloud_panel.annotations.clone();let selected=studio.cloud_panel.selected_snapshot.clone();
   egui::ScrollArea::vertical().max_height(510.).show(ui,|ui|{
    if studio.cloud_panel.preview_file==selected {if let Some(texture)=&studio.cloud_panel.preview{let size=texture.size_vec2();let scale=(ui.available_width().min(700.)/size.x).min(320./size.y);let response=ui.image((texture.id(),size*scale));for (index,a) in annotations.iter().filter(|a|value(a,"snapshotId")==selected).enumerate(){super::cloud_annotations::paint(ui,response.rect,a,index);}}}
    for (index,a) in annotations.iter().filter(|a|value(a,"snapshotId")==selected).enumerate(){ui.separator();ui.label(format!("{} · {} · {}",index+1,value(a,"authorName"),if a["resolved"]==true{"Resolved"}else{"Open"}));ui.label(value(a,"body"));if let Some(replies)=a["replies"].as_array(){for reply in replies{ui.label(format!("{}: {}",value(reply,"authorName"),value(reply,"body")));}}
     ui.horizontal(|ui|{let id=value(a,"_id").to_string();let resolved=a["resolved"]!=true;if action(ui,owner.id.with(("resolve-thread",&id)),if resolved{"Resolve"}else{"Reopen"},&format!("{} review thread by {}: {}",if resolved{"Resolve"}else{"Reopen"},value(a,"authorName"),value(a,"body")),!busy).clicked(){studio.cloud_task(move|c|{c.call("review:resolve",json!({"id":id,"resolved":resolved}))?;Ok(Event::Notice("Thread updated. Refresh to load the latest review.".into()))});}
      if action(ui,owner.id.with(("reply-thread",value(a,"_id"))),"Reply with draft",&format!("Reply to review thread by {}: {}",value(a,"authorName"),value(a,"body")),!busy&&valid_text(&studio.cloud_panel.reply,4000)).clicked(){let id=value(a,"_id").to_string();let body=studio.cloud_panel.reply.clone();studio.cloud_task(move|c|{c.call("review:reply",json!({"id":id,"body":body}))?;Ok(Event::Notice("Reply posted. Refresh to load the latest review.".into()))});studio.cloud_panel.reply.clear();}
     });
    }
    let reply=super::agent::text(ui,owner.id.with(("reply",&selected)),"Cloud reply draft",&mut studio.cloud_panel.reply,true);
    if !studio.cloud_panel.reply.is_empty()&&!valid_text(&studio.cloud_panel.reply,4000){super::welcome::control_error(ui,&[&reply],"Reply must contain 1 to 4000 characters");}
    ui.separator();ui.heading("Public showcase");ui.label("Only the selected flat export becomes public. Source files and assets stay private.");
    let title=super::agent::text(ui,owner.id.with(("publish-title",&selected)),"Showcase work title",&mut studio.publish_title,false);
    let summary=super::agent::text(ui,owner.id.with(("publish-summary",&selected)),"Showcase description",&mut studio.publish_summary,true);
    let title_valid=valid_text(&studio.publish_title,200);let summary_valid=valid_text(&studio.publish_summary,2000);
    if !title_valid{super::welcome::control_error(ui,&[&title],"Title must contain 1 to 200 characters");}
    if !summary_valid{super::welcome::control_error(ui,&[&summary],"Description must contain 1 to 2000 characters");}
    if action(ui,owner.id.with("publish"),"Publish selected export","Publish selected cloud export",!busy&&snapshot_valid&&title_valid&&summary_valid).clicked(){studio.publish_showcase();}
    let works=studio.cloud_panel.showcase.clone();let competitions=studio.cloud_panel.competitions.clone();
    for work in works.iter().filter(|w|w["published"]==true){ui.label(value(work,"title"));if action(ui,owner.id.with(("unpublish",value(work,"_id"))),"Unpublish",&format!("Unpublish showcase work {}",value(work,"title")),!busy).clicked(){let id=value(work,"_id").to_string();studio.cloud_task(move|c|{c.call("showcase:unpublish",json!({"id":id}))?;Ok(Event::Notice("Work unpublished. Refresh to update the list.".into()))});}for contest in &competitions{if action(ui,owner.id.with(("competition",value(work,"_id"),value(contest,"_id"))),&format!("Submit to {}",value(contest,"title")),&format!("Submit {} to {}",value(work,"title"),value(contest,"title")),!busy).clicked(){let id=value(contest,"_id").to_string();let work=value(work,"_id").to_string();studio.cloud_task(move|c|{c.call("showcase:enter",json!({"competitionId":id,"showcaseId":work}))?;Ok(Event::Notice("Competition entry submitted".into()))});}}}
    if competitions.is_empty(){ui.label("No competitions are open yet.");}
   });
  },
  CloudModal::None=>{},
 }
 ui.separator();if busy{ui.spinner();}let status=ui.label(&studio.status);ui.ctx().accesskit_node_builder(status.id,|node|{node.set_live(egui::accesskit::Live::Polite);node.set_live_atomic();});if action(ui,owner.id.with("close"),"Close","Close cloud dialog",true).clicked(){studio.cloud_modal=CloudModal::None;}
 });
    if studio.cloud_busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }
    if dialog.should_close() {
        studio.cloud_modal = CloudModal::None;
    }
    if studio.cloud_modal==CloudModal::None{
        ctx.data_mut(|data|data.remove::<Owner>(egui::Id::new("cloud-native-owner")));
        super::panel_workspace::return_focus(&ctx,studio);
    }else{ctx.data_mut(|data|data.insert_temp(egui::Id::new("cloud-native-owner"),owner));}
}

fn live_ui(ui: &mut Ui, studio: &mut Studio, busy: bool) {
    if let Some(replica) = &studio.doc.collaboration {
        let session = replica.session.clone();
        let paused = replica.paused;
        let pending = replica.pending.len();
        let conflicts = studio.live_conflict_count();
        ui.label(studio.live_status());
        ui.horizontal(|ui| {
            ui.monospace(&session);
            if ui.button("Copy session ID").clicked() { ui.ctx().copy_text(session); }
        });
        ui.label("Share this ID with a project editor. Reviewers cannot open the editable document.");
        if ui.button(if paused { "Reconnect" } else { "Work offline" }).clicked() { studio.pause_live(!paused); }
        ui.label(format!("{pending} edits waiting to send. Save the .oma file to keep offline edits and your undo history."));
        for peer in studio.live_presence() { ui.label(format!("{} · {} selected", peer.name, peer.selection.len())); }
        if conflicts > 0 {
            ui.label(format!("{conflicts} properties have competing edits. Both edits remain saved. Undoing your edit reveals the other editor's value when it is still applicable."));
        }
    } else {
        ui.label("Edit the same native document together. Each editor keeps their own undo history; offline edits merge when they reconnect.");
        if ui.add_enabled(!busy && cloud::connected(&studio.cloud_identity), egui::Button::new("Start live edits for this project")).clicked() { studio.start_live(); }
        ui.label("Start requires a cloud project. Manage editor invitations on the cloud website.");
        ui.separator();
        ui.label("Session ID");
        ui.text_edit_singleline(&mut studio.live_join_id);
        if ui.add_enabled(!busy && cloud::connected(&studio.cloud_identity) && !studio.live_join_id.trim().is_empty(), egui::Button::new("Join in a new document")).clicked() { studio.join_live(); }
    }
}

pub fn paint_presence(painter: &egui::Painter, rect: egui::Rect, studio: &Studio) {
    let painter = painter.with_clip_rect(rect);
    for peer in studio.live_presence() {
        let hue = peer.actor.bytes().fold(0u8, u8::wrapping_add);
        let color = egui::Color32::from_rgb(100 + hue / 2, 210 - hue / 3, 220);
        let screen = |point: crate::geom::Pt| {
            let point = studio.view.to_screen(point);
            egui::pos2(rect.min.x + point.x, rect.min.y + point.y)
        };
        if let Some(cursor) = peer.cursor {
            let point = screen(cursor);
            painter.circle_filled(point, 4., color);
            painter.text(point + egui::vec2(8., -5.), egui::Align2::LEFT_BOTTOM, &peer.name, egui::FontId::proportional(12.), color);
        }
        for id in &peer.selection {
            let Ok(id) = id.parse::<u64>() else { continue; };
            for shape in studio.doc.layers.iter().flat_map(|layer| layer.kind.shapes().unwrap_or_default()) {
                if shape.id == id {
                    let bounds = shape.world_bbox();
                    painter.rect_stroke(egui::Rect::from_min_max(screen(bounds.min), screen(bounds.max)), 0., egui::Stroke::new(1., color), egui::StrokeKind::Outside);
                }
            }
        }
    }
}
