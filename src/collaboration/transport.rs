//! Bounded authenticated exchanges; the editor owns retries and its durable queue.

use super::{Change, Id, PROTOCOL, Replica};
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
        self.call(
            "live:create",
            json!({"projectId":project,"seed":seed,"protocol":PROTOCOL}),
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
            json!({"session":session,"actor":actor,"protocol":PROTOCOL}),
        )?;
        if joined["supportedProtocol"].as_u64() != Some(PROTOCOL as u64)
            || joined["protocol"]
                .as_u64()
                .is_none_or(|n| n > PROTOCOL as u64)
        {
            return Err("Live service does not negotiate native preservation".into());
        }
        let seed = joined["seed"]
            .as_str()
            .ok_or("Missing live document seed")?;
        let base = serde_json::from_str(seed).map_err(|e| format!("Invalid live seed: {e}"))?;
        let mut replica = Replica::new(session.into(), actor.into(), base);
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
        serde_json::from_value(self.call("live:exchange", json!({"session":replica.session,"actor":replica.actor,"changes":changes,"cursor":cursor,"selection":selection,"after":replica.cursor,"protocol":PROTOCOL}))?).map_err(|e| format!("Invalid live exchange: {e}"))
    }
}
