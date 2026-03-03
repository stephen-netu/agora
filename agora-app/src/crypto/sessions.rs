//! Session management helpers for E2EE

use vodozemac::megolm::{
    GroupSession as OutboundGroupSession, GroupSessionPickle, InboundGroupSession,
    InboundGroupSessionPickle,
};
use vodozemac::olm::{Session as OlmSession, SessionPickle};

/// Pickle an Olm session for storage
pub fn pickle_olm_session(session: &OlmSession) -> String {
    serde_json::to_string(&session.pickle()).unwrap_or_default()
}

/// Unpickle an Olm session from storage
pub fn unpickle_olm_session(s: &str) -> Result<OlmSession, String> {
    let pickle: SessionPickle =
        serde_json::from_str(s).map_err(|e| format!("unpickle olm session: {e}"))?;
    Ok(OlmSession::from_pickle(pickle))
}

/// Pickle an outbound group session for storage
pub fn pickle_outbound_group(session: &OutboundGroupSession) -> String {
    serde_json::to_string(&session.pickle()).unwrap_or_default()
}

/// Unpickle an outbound group session from storage
pub fn unpickle_outbound_group(s: &str) -> Result<OutboundGroupSession, String> {
    let pickle: GroupSessionPickle =
        serde_json::from_str(s).map_err(|e| format!("unpickle outbound group: {e}"))?;
    Ok(OutboundGroupSession::from_pickle(pickle))
}

/// Pickle an inbound group session for storage
pub fn pickle_inbound_group(session: &InboundGroupSession) -> String {
    serde_json::to_string(&session.pickle()).unwrap_or_default()
}

/// Unpickle an inbound group session from storage
pub fn unpickle_inbound_group(s: &str) -> Result<InboundGroupSession, String> {
    let pickle: InboundGroupSessionPickle =
        serde_json::from_str(s).map_err(|e| format!("unpickle inbound group: {e}"))?;
    Ok(InboundGroupSession::from_pickle(pickle))
}
