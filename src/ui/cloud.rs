use crate::{
    app::{CloudModal, Studio},
    cloud::{self, client::Event},
};
use eframe::egui::{self, Ui};
use serde_json::json;
fn value<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn modal(ui: &mut Ui, studio: &mut Studio) {
    if studio.cloud_modal == CloudModal::None {
        return;
    }
    let ctx = ui.ctx().clone();
    let title = match studio.cloud_modal {
        CloudModal::SignIn => "Connect to cloud",
        CloudModal::Projects => "Cloud projects",
        CloudModal::Review => "Review & annotations",
        CloudModal::Invite => "Invite a reviewer",
        CloudModal::Publish => "Publish finished work",
        CloudModal::None => return,
    };
    let dialog=egui::Modal::new(egui::Id::new("cloud-modal")).show(&ctx,|ui|{
 ui.set_width(if matches!(studio.cloud_modal,CloudModal::Review|CloudModal::Publish){760.}else{440.});ui.heading(title);ui.add_space(10.);
 let busy=studio.cloud_busy();
 match studio.cloud_modal {
  CloudModal::SignIn=>{
   if cloud::signed_in(&studio.cloud_identity){ui.label(format!("Connected as {}",studio.cloud_identity.email));if ui.button("Disconnect this desktop").clicked(){studio.cloud_identity=Default::default();let _=cloud::save_identity(&studio.cloud_identity);studio.status="Disconnected locally. Revoke the device from Account on the website.".into();}}
   else{ui.label("Sign in securely in your browser, then approve this desktop. Your project files stay local until you push them.");if !studio.cloud_panel.code.is_empty(){ui.heading(&studio.cloud_panel.code);ui.label("Check that this code matches the code in your browser.");}if ui.add_enabled(!busy,egui::Button::new("Open browser to sign in")).clicked(){studio.connect_cloud();}}
  },
  CloudModal::Projects=>{
   if ui.add_enabled(!busy,egui::Button::new("Refresh projects")).clicked(){studio.show_cloud_projects();}
   let projects=studio.cloud_panel.projects.clone();egui::ScrollArea::vertical().max_height(480.).show(ui,|ui|{for project in projects{ui.horizontal(|ui|{ui.label(format!("{} · {}",project.title,project.role));if ui.add_enabled(!busy&&project.role!="reviewer",egui::Button::new("Pull & open")).clicked(){studio.pull_cloud_project(project.id);}});}});
   ui.label("Pull opens a separate document and preserves your current work.");
  },
  CloudModal::Invite=>{ui.label("Reviewers can see flat exports and comments. Manage editor access from the cloud project on the website.");ui.text_edit_singleline(&mut studio.invite_email);if ui.add_enabled(!busy,egui::Button::new("Send review invitation")).clicked(){studio.invite_collaborator();}},
  CloudModal::Review|CloudModal::Publish=>{
   ui.horizontal(|ui|{if ui.add_enabled(!busy,egui::Button::new("Refresh cloud review")).clicked(){studio.refresh_cloud_review();}if ui.button("Open web project ↗").clicked(){if let Some(link)=&studio.doc.cloud{let _=std::process::Command::new("xdg-open").arg(format!("https://omadesign.app/cloud?project={}",link.project_id)).spawn();}}});
   let snapshots:Vec<_>=studio.cloud_panel.files.iter().filter(|f|f.kind=="snapshot").cloned().collect();
   egui::ComboBox::from_id_salt("cloud-snapshot").selected_text(snapshots.iter().find(|f|f.id==studio.cloud_panel.selected_snapshot).map(|f|format!("{} · v{}",f.name,f.version)).unwrap_or_else(||"Choose a flat export".into())).show_ui(ui,|ui|{for f in &snapshots{ui.selectable_value(&mut studio.cloud_panel.selected_snapshot,f.id.clone(),format!("{} · v{}",f.name,f.version));}});
   if ui.add_enabled(!busy&&!studio.cloud_panel.selected_snapshot.is_empty(),egui::Button::new("View selected export")).clicked(){studio.load_cloud_preview();}
   let annotations=studio.cloud_panel.annotations.clone();let selected=studio.cloud_panel.selected_snapshot.clone();
   egui::ScrollArea::vertical().max_height(510.).show(ui,|ui|{
    if studio.cloud_panel.preview_file==selected {if let Some(texture)=&studio.cloud_panel.preview{let size=texture.size_vec2();let width=ui.available_width().min(700.);let response=ui.image((texture.id(),size*(width/size.x)));for (index,a) in annotations.iter().filter(|a|value(a,"snapshotId")==selected).enumerate(){let x=a["x"].as_f64().unwrap_or(0.) as f32;let y=a["y"].as_f64().unwrap_or(0.) as f32;let point=response.rect.min+response.rect.size()*egui::vec2(x,y);ui.painter().circle_filled(point,11.,egui::Color32::from_rgb(213,255,114));ui.painter().text(point,egui::Align2::CENTER_CENTER,(index+1).to_string(),egui::FontId::proportional(12.),egui::Color32::BLACK);}}}
    for (index,a) in annotations.iter().filter(|a|value(a,"snapshotId")==selected).enumerate(){ui.separator();ui.label(format!("{} · {} · {}",index+1,value(a,"authorName"),if a["resolved"]==true{"Resolved"}else{"Open"}));ui.label(value(a,"body"));if let Some(replies)=a["replies"].as_array(){for reply in replies{ui.label(format!("{}: {}",value(reply,"authorName"),value(reply,"body")));}}
     ui.horizontal(|ui|{let id=value(a,"_id").to_string();let resolved=a["resolved"]!=true;if ui.add_enabled(!busy,egui::Button::new(if resolved{"Resolve"}else{"Reopen"})).clicked(){studio.cloud_task(move|c|{c.call("review:resolve",json!({"id":id,"resolved":resolved}))?;Ok(Event::Notice("Thread updated. Refresh to load the latest review.".into()))});}
      if ui.add_enabled(!busy&&!studio.cloud_panel.reply.trim().is_empty(),egui::Button::new("Reply with draft")).clicked(){let id=value(a,"_id").to_string();let body=studio.cloud_panel.reply.clone();studio.cloud_task(move|c|{c.call("review:reply",json!({"id":id,"body":body}))?;Ok(Event::Notice("Reply posted. Refresh to load the latest review.".into()))});studio.cloud_panel.reply.clear();}
     });
    }
    ui.label("Reply draft");ui.text_edit_multiline(&mut studio.cloud_panel.reply);
    ui.separator();ui.heading("Public showcase");ui.label("Only the selected flat export becomes public. Source files and assets stay private.");ui.add(egui::TextEdit::singleline(&mut studio.publish_title).hint_text("Work title"));ui.add(egui::TextEdit::multiline(&mut studio.publish_summary).hint_text("Description"));
    if ui.add_enabled(!busy&&!selected.is_empty(),egui::Button::new("Publish selected export")).clicked(){studio.publish_showcase();}
    let works=studio.cloud_panel.showcase.clone();let competitions=studio.cloud_panel.competitions.clone();
    for work in works.iter().filter(|w|w["published"]==true){ui.label(value(work,"title"));if ui.add_enabled(!busy,egui::Button::new("Unpublish")).clicked(){let id=value(work,"_id").to_string();studio.cloud_task(move|c|{c.call("showcase:unpublish",json!({"id":id}))?;Ok(Event::Notice("Work unpublished. Refresh to update the list.".into()))});}for contest in &competitions{if ui.add_enabled(!busy,egui::Button::new(format!("Submit to {}",value(contest,"title")))).clicked(){let id=value(contest,"_id").to_string();let work=value(work,"_id").to_string();studio.cloud_task(move|c|{c.call("showcase:enter",json!({"competitionId":id,"showcaseId":work}))?;Ok(Event::Notice("Competition entry submitted".into()))});}}}
    if competitions.is_empty(){ui.label("No competitions are open yet.");}
   });
  },
  CloudModal::None=>{},
 }
 ui.separator();if busy{ui.spinner();}ui.label(&studio.status);if ui.button("Close").clicked(){studio.cloud_modal=CloudModal::None;}
 });
    if dialog.should_close() {
        studio.cloud_modal = CloudModal::None;
    }
}
