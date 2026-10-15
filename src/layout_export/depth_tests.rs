use super::*;
use crate::document::{Layer, Style};
use crate::layout::{LayoutBreakpoint, make_frame};
use crate::layout_prototype::{Interaction, PrototypeAction as Action, Transition, Trigger};

fn interaction(action: Action) -> Interaction {
    Interaction { action, trigger: Trigger::Click, transition: Transition::Instant, duration_ms: 0 }
}
fn button(doc: &mut Document, layer: usize, parent: u64, name: &str, action: Action, x: f32, y: f32) -> u64 {
    let mut node=crate::document::Shape::new(Geom::Rect{origin:Pt::new(x,y),size:Pt::new(72.,24.),radius:0.},Style::default());
    node.name=name.into();node.layout.parent=Some(parent);node.layout.interactions.push(interaction(action));
    let id=node.id;doc.layers[layer].kind.shapes_mut().unwrap().push(node);id
}
fn graph() -> (Document, u64, u64, u64, u64) {
    let mut doc=Document::new("Prototype C/L/P/V",600.,400.,96.);
    doc.layers=vec![Layer::vector("A"),Layer::vector("B"),Layer::vector("Overlays")];
    let mut a=make_frame(Pt::ZERO,Pt::new(600.,400.));a.name="Screen A".into();
    let mut b=make_frame(Pt::new(700.,0.),Pt::new(600.,400.));b.name="Screen B".into();
    let mut o1=make_frame(Pt::new(0.,500.),Pt::new(240.,160.));o1.name="Overlay one".into();
    let mut o2=make_frame(Pt::new(300.,500.),Pt::new(160.,100.));o2.name="Overlay two".into();
    let ids=(a.id,b.id,o1.id,o2.id);
    doc.layers[0].kind.shapes_mut().unwrap().push(a);doc.layers[1].kind.shapes_mut().unwrap().push(b);
    doc.layers[2].kind.shapes_mut().unwrap().extend([o1,o2]);
    button(&mut doc,0,ids.0,"Go B",Action::Navigate{target:ids.1},10.,10.);
    button(&mut doc,0,ids.0,"Open one",Action::OpenOverlay{target:ids.2},10.,50.);
    button(&mut doc,0,ids.0,"Self",Action::Navigate{target:ids.0},10.,90.);
    button(&mut doc,1,ids.1,"Back A",Action::Back,710.,10.);
    button(&mut doc,2,ids.2,"Open two",Action::OpenOverlay{target:ids.3},10.,510.);
    button(&mut doc,2,ids.2,"Close one",Action::CloseOverlay,10.,550.);
    button(&mut doc,2,ids.3,"Back one",Action::Back,310.,510.);
    let mut private=make_frame(Pt::new(900.,900.),Pt::new(100.,100.));private.name="PRIVATE SECRET".into();
    doc.layers[2].kind.shapes_mut().unwrap().push(private);
    (doc,ids.0,ids.1,ids.2,ids.3)
}
#[test]
fn document_wide_reachable_graph_reopens_without_private_siblings_and_invalid_links_fail() {
    let (doc,a,b,o1,o2)=graph();
    let before=crate::project::encode(&doc).unwrap();let reopened=crate::project::decode(&before).unwrap();
    let html=export_html(&reopened,0,a).unwrap();
    for id in [a,b,o1,o2] { assert!(html.contains(&format!("id=\"screen-{id}\""))); }
    assert!(!html.contains("PRIVATE SECRET"));assert_eq!(html.matches("<template ").count(),4);
    assert_eq!(html,export_html(&reopened,0,a).unwrap());assert_eq!(before,crate::project::encode(&doc).unwrap());
    let mut broken=reopened;broken.find_shape_mut(0,a).unwrap().layout.interactions.push(interaction(Action::Navigate{target:u64::MAX}));
    assert!(export_html(&broken,0,a).unwrap_err().contains("Invalid prototype link"));
}
#[test]
fn conflicting_variant_rules_are_scoped_to_emitted_state_and_export_deterministically() {
    let (mut doc,a,_,_,_)=graph();
    let mut main=make_frame(Pt::new(0.,1000.),Pt::new(180.,80.));
    main.layout.stack=Some(crate::layout::AutoStack::default());
    main.layout.breakpoints.push(LayoutBreakpoint{max_width:600.,gap:Some(7.),..Default::default()});
    let mid=main.id;doc.layers[2].kind.shapes_mut().unwrap().push(main);
    crate::layout_components::make_component(&mut doc,2,mid).unwrap();
    let v1=crate::layout_components::add_variant(&mut doc,mid,"Compact",Pt::new(250.,1000.)).unwrap();
    let v2=crate::layout_components::add_variant(&mut doc,mid,"Wide",Pt::new(500.,1000.)).unwrap();
    for (id,gap,axis) in [(v1,31.,StackAxis::Horizontal),(v2,53.,StackAxis::Vertical)]{
        let shape=doc.find_shape_mut(2,id).unwrap();shape.layout.breakpoints=vec![LayoutBreakpoint{max_width:600.,gap:Some(gap),direction:Some(axis),..Default::default()}];
    }
    let instance=crate::layout_components::insert_instance(&mut doc,mid,0,Pt::new(120.,100.),Some(a)).unwrap();
    doc.find_shape_mut(0,instance).unwrap().layout.interactions=vec![interaction(Action::SetVariant{component:v1}),Interaction{trigger:Trigger::Hover,..interaction(Action::SetVariant{component:v2})}];
    let reopened=crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let html=export_html(&reopened,0,a).unwrap();assert_eq!(html,export_html(&reopened,0,a).unwrap());
    assert!(html.contains(&format!("[data-state=\"variant-{instance}-{v1}\"][data-node=\"{instance}\"]")));
    assert!(!html.contains(&format!("{{[data-node=\"{instance}\"]")));
    assert!(!html.contains(&format!("id=\"screen-{v1}\"")));
    if let Ok(directory)=std::env::var("OMADESIGN_LAYOUT_PROOF") {
        let path=std::path::Path::new(&directory);assert!(path.is_absolute()&&directory.starts_with("/home/"));std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("prototype.html"),&html).unwrap();
        std::fs::write(path.join("prototype.oma"),crate::project::encode(&reopened).unwrap()).unwrap();
        std::fs::write(path.join("ids.json"),serde_json::json!({"a":a,"instance":instance,"compact":v1,"wide":v2}).to_string()).unwrap();
    }
}
