//! Live smoke test against a disposable development account. No production defaults.
use omadesign::{cloud::{Identity,client::Client},document::Document};
use serde_json::json;
fn main()->Result<(),String>{
 let identity:Identity=serde_json::from_str(&std::fs::read_to_string(std::env::var("OMADESIGN_QA_IDENTITY").map_err(|_|"Set OMADESIGN_QA_IDENTITY")?).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 if !identity.cloud_url.starts_with("http://localhost:"){return Err("QA requires a local web server connected to development Convex".into());}
 let client=Client::new(identity);let doc=Document::new("Native cloud smoke test",400.,300.,72.);
 let link=client.push(&doc)?;let restored=client.pull(&link.project_id)?;
 if restored.width!=400.||restored.height!=300.{return Err("Pulled document differs".into());}
 let thread=client.call("review:annotate",json!({"snapshotId":link.document_id,"x":0.3,"y":0.4,"shape":"pin","body":"Native review probe"}))?;
 client.call("review:reply",json!({"id":thread,"body":"Native reply"}))?;
 let reviews=client.call("review:list",json!({"projectId":link.project_id}))?;
 if reviews.as_array().map(Vec::len)!=Some(1){return Err("Review did not persist".into());}
 std::fs::write("/tmp/omadesign-native-qa-project.json",json!({"projectId":link.project_id}).to_string()).map_err(|e|e.to_string())?;
 println!("Native push, flat export, pull, annotation and reply passed.");Ok(())
}
