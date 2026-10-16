//! Bounded authenticated exchanges; the editor owns retries and its durable queue.

use super::{Change, Id, Replica, protocol};
use crate::cloud::client::Client;
use crate::geom::Pt;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Debug, Deserialize)]
pub struct Presence {
    pub actor: String,
    pub name: String,
    pub cursor: Option<Pt>,
    pub selection: Vec<String>,
}

#[derive(Deserialize)]
pub struct Reply {
    #[serde(default)]
    pub protocol: Option<u32>,
    #[serde(default, rename = "supportedProtocol")]
    pub supported_protocol: Option<u32>,
    pub acknowledged: Vec<Id>,
    pub changes: Vec<Change>,
    pub cursor: u64,
    pub more: bool,
    pub presence: Vec<Presence>,
}

impl Client {
    /// Publish a seed only to a service that understands explicit native capability arguments.
    /// Args: project ID and complete seed. Returns a session ID or refusal without local mutation.
    pub fn create_live(&self, project: &str, seed: String) -> Result<String, String> {
        let base = serde_json::from_str(&seed).map_err(|e| format!("Invalid live seed: {e}"))?;
        protocol::require(
            &Replica::new(String::new(), String::new(), base),
            protocol::local_maximum(),
        )?;
        self.call(
            "live:create",
            json!({"projectId":project,"seed":seed,"protocol":protocol::local_maximum()}),
        )?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Missing live session ID".into())
    }

    /// Register or resume an authenticated client without replacing any local work.
    /// Args: session ID and actor ID. Returns the shared immutable document seed.
    pub fn join_live(&self, session: &str, actor: &str) -> Result<Replica, String> {
        let joined = self.call(
            "live:join",
            json!({"session":session,"actor":actor,"protocol":protocol::local_maximum()}),
        )?;
        if joined["supportedProtocol"].as_u64() != Some(protocol::local_maximum() as u64)
            || joined["protocol"]
                .as_u64()
                .is_none_or(|n| !(1..=protocol::local_maximum() as u64).contains(&n))
        {
            return Err("Live service does not negotiate native preservation".into());
        }
        let seed = joined["seed"]
            .as_str()
            .ok_or("Missing live document seed")?;
        let base = serde_json::from_str(seed).map_err(|e| format!("Invalid live seed: {e}"))?;
        let mut replica = Replica::new(session.into(), actor.into(), base);
        let required = protocol::require(&replica, protocol::local_maximum())?;
        if joined["protocol"]
            .as_u64()
            .is_none_or(|n| n < required as u64)
        {
            return Err("Live service understated the retained capability".into());
        }
        replica.project = joined["projectId"]
            .as_str()
            .ok_or("Missing live project ID")?
            .into();
        Ok(replica)
    }

    /// Send a bounded prefix of pending operations and receive the next durable page.
    /// Args: saved replica, current presence, reconnect flag. Returns an acknowledged page.
    pub fn exchange_live(
        &self,
        replica: &Replica,
        cursor: Option<Pt>,
        selection: Vec<String>,
        reconnect: bool,
    ) -> Result<Reply, String> {
        protocol::require(replica, protocol::local_maximum())?;
        if reconnect {
            self.join_live(&replica.session, &replica.actor)?;
        }
        let mut changes = Vec::new();
        let mut bytes = 2;
        let mut pending: Vec<_> = replica
            .changes
            .iter()
            .filter(|c| replica.pending.contains(&c.id))
            .collect();
        pending.sort_by_key(|c| c.id.seq);
        for change in pending.into_iter().take(64) {
            let length = serde_json::to_vec(change).map_err(|e| e.to_string())?.len() + 1;
            if length > 190_000 {
                return Err("A live edit exceeds 190 KB. Save locally; snapshot sharing retains the complete artwork.".into());
            }
            if bytes + length > 190_000 {
                break;
            }
            changes.push(change);
            bytes += length;
        }
        let reply: Reply = serde_json::from_value(self.call("live:exchange", json!({"session":replica.session,"actor":replica.actor,"changes":changes,"cursor":cursor,"selection":selection,"after":replica.cursor,"protocol":protocol::local_maximum()}))?).map_err(|e| format!("Invalid live exchange: {e}"))?;
        validate_reply(replica, &reply)?;
        Ok(reply)
    }
}

fn validate_reply(replica: &Replica, reply: &Reply) -> Result<(), String> {
    let mut required = replica.protocol_requirement()?;
    for change in &reply.changes {
        if let super::Edit::Write { writes } = &change.edit {
            for write in writes {
                if let Some(cell) = &write.value {
                    required = required.max(protocol::cell(&write.path, cell)?);
                }
            }
        }
    }
    protocol::require_minimum(required, protocol::local_maximum())?;
    if reply
        .protocol
        .is_some_and(|n| n < required || n > protocol::local_maximum())
        || reply
            .supported_protocol
            .is_some_and(|n| n != protocol::local_maximum())
        || (protocol::local_maximum() == 3
            && (reply.protocol.is_none() || reply.supported_protocol.is_none()))
    {
        return Err("Live service did not negotiate the retained capability".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_gate_checks_empty_polls_and_retained_changes_before_ack_or_presence() {
        let document = crate::document::Document::new("Reply", 1., 1., 72.);
        let replica = Replica::new(
            "session".into(),
            "a".repeat(64),
            crate::collaboration::snapshot(&document).unwrap(),
        );
        let before = serde_json::to_vec(&replica).unwrap();
        let reply = |protocol, supported, changes| {
            serde_json::from_value::<Reply>(json!({
                "protocol":protocol,"supportedProtocol":supported,
                "acknowledged":[],"changes":changes,"cursor":0,"more":false,"presence":[]
            }))
            .unwrap()
        };
        assert!(validate_reply(&replica, &reply(1, protocol::local_maximum(), json!([]))).is_ok());
        if protocol::local_maximum() == 2 {
            assert!(validate_reply(&replica, &reply(3, 2, json!([]))).is_err());
            assert!(validate_reply(&replica, &reply(1, 3, json!([]))).is_err());
            let advanced = json!([{"id":{"actor":"remote","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":"/publishing/facing_pages","value":{"Value":true}}]}}]);
            assert!(validate_reply(&replica, &reply(1, 2, advanced)).is_err());
        }
        assert_eq!(serde_json::to_vec(&replica).unwrap(), before);
    }
}
