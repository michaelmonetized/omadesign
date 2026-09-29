//! Native path-type actions share the document transaction/history machinery.
use super::*;
use crate::text_geometry::{TextOnPath,guide_path};
impl Studio {
    pub fn text_path_target(&self, point: Pt, slack: f32) -> Option<(usize,u64)> {
        self.doc.layers.iter().enumerate().rev().filter(|(_,l)|l.visible&&!l.locked).find_map(|(li,l)| {
            l.kind.shapes()?.iter().rev().filter(|s|s.visible&&!s.locked).find_map(|s| {
                let path=guide_path(s)?;
                let distance=path.nearest_distance(point);
                ((path.point_and_tangent_at(distance).0-point).length()<=slack).then_some((li,s.id))
            })
        })
    }
    pub fn place_text_on_path(&mut self, at:Pt, guide:(usize,u64)) {
        self.place_text(at);
        let Some(&(layer,id))=self.selection.first() else{return};
        let Some(shape)=self.doc.find_shape(guide.0,guide.1) else{return};
        let Some(path)=guide_path(shape) else{return};
        let start=path.nearest_distance(at)/path.length.max(0.001);
        if let Some(shape)=self.doc.find_shape_mut(layer,id) && let Geom::Text(run)=&mut shape.geom {
            run.on_path=Some(TextOnPath {path_id:guide.1,start,end:if path.closed{start+1.}else{1.},cache:Some(path),..Default::default()});
            run.wrap_width=None;run.layout=None;
        }
        self.mark();
        self.status="type on path — edit the text or drag its brackets".into();
    }
    pub fn can_attach_text_path(&self)->bool {
        self.selection.len()==2 && self.selection.iter().filter_map(|&(l,id)|self.doc.find_shape(l,id)).filter(|s|matches!(s.geom,Geom::Text(_))).count()==1 && self.selection.iter().filter_map(|&(l,id)|self.doc.find_shape(l,id)).any(|s|guide_path(s).is_some())
    }
    pub fn attach_text_path(&mut self) {
        self.commit_type_edit();
        if !self.can_attach_text_path(){return;}
        let Some((layer,id,before,rotation))=self.selection.iter().find_map(|&(l,id)| {
            let s=self.doc.find_shape(l,id)?;
            matches!(s.geom,Geom::Text(_)).then_some((l,id,s.geom.clone(),s.rotation))
        }) else{return};
        let Some((guide_id,path))=self.selection.iter().filter_map(|&(l,id)|self.doc.find_shape(l,id)).find_map(|s|guide_path(s).map(|path|(s.id,path))) else{return};
        let mut after=before.clone();
        if let Geom::Text(run)=&mut after {run.on_path=Some(TextOnPath{path_id:guide_id,cache:Some(path),..Default::default()});run.wrap_width=None;run.layout=None;}
        self.commit(Cmd::SetGeom{layer,id,before,after,rot_before:rotation,rot_after:0.});
        self.selection=vec![(layer,id)];
        self.status="text attached to path".into();
    }
    pub fn release_text_path(&mut self) {
        self.commit_type_edit();
        self.patch_type(|run|crate::text_geometry::release(run));
        self.status="released as editable point text".into();
    }
}
