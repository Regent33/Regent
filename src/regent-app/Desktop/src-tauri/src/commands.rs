//! The single IPC seam. The webview has no shell/fs/http capability — every
//! backend action is a validated JSON-RPC request forwarded to the deacon here.

use crate::deacon::DeaconState;
use serde_json::Value;
use tauri::State;

/// Methods are `namespace.method`, lowercase ASCII + underscores, exactly one
/// dot — the deacon dispatcher's contract (`^[a-z_]+\.[a-z_]+$`). Reject
/// anything else at the boundary before it reaches the pipe.
fn valid_method(method: &str) -> bool {
    let Some((ns, name)) = method.split_once('.') else {
        return false;
    };
    let segment_ok =
        |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
    segment_ok(ns) && segment_ok(name)
}

/// Forward a validated request to the deacon and return its raw JSON-RPC
/// response. Errors come back as strings; presentation formats them later.
#[tauri::command]
pub async fn deacon_request(
    state: State<'_, DeaconState>,
    method: String,
    params: Value,
) -> Result<Value, String> {
    if !valid_method(&method) {
        return Err(format!("invalid method: {method}"));
    }
    if !params.is_object() {
        return Err("params must be a JSON object".into());
    }
    let Some(rpc) = state.client().await else {
        return Err("deacon is not running".into());
    };
    rpc.request(&method, params).await
}

#[cfg(test)]
mod tests {
    use super::valid_method;

    #[test]
    fn accepts_namespaced_methods_only() {
        assert!(valid_method("status.get"));
        assert!(valid_method("code.plan"));
        assert!(valid_method("sessions.list"));
        assert!(!valid_method("statusget")); // no dot
        assert!(!valid_method("status.")); // empty method
        assert!(!valid_method(".get")); // empty namespace
        assert!(!valid_method("a.b.c")); // two dots
        assert!(!valid_method("Status.Get")); // uppercase
        assert!(!valid_method("status.get-all")); // hyphen
    }
}
