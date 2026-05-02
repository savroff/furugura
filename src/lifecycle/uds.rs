//! UDS protocol message types and (de)serialization.
//!
//! The orchestrator binds a Unix-domain socket at
//! `$XDG_RUNTIME_DIR/furugura/furugura.sock` (mode 0600). Messages are
//! line-delimited JSON, one request per line, one response per line.
//!
//! This file defines the wire format only. The server loop lives in the
//! orchestrator (cli/start.rs) and the client lives in cli/mark.rs and
//! cli/stop.rs.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Request {
    /// Append a marker line to `notes.live`.
    Mark {
        text: String,
        /// Display timestamp `HH:MM:SS` from the marker's perspective.
        t: String,
    },
    /// Transition the meeting from `capturing` to `finalizing`.
    Stop,
    /// Return current state + elapsed seconds since start.
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "ok")]
pub enum Response {
    #[serde(rename = "true")]
    Ok(OkPayload),
    #[serde(rename = "false")]
    Err { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OkPayload {
    // NB: variant order matters for `untagged` deserialization.
    // The most-specific variants (most required fields) must come first.
    /// Reply to `Status`.
    Status {
        state: String,
        elapsed_seconds: i64,
    },
    /// Reply to `Mark`.
    MarkAck { appended: String },
    /// Reply to `Stop`. Carries the new state ("finalizing").
    StopAck { state: String },
}

pub fn parse_request(line: &str) -> Result<Request> {
    serde_json::from_str(line.trim()).context("invalid UDS request JSON")
}

pub fn render_response(resp: &Response) -> String {
    serde_json::to_string(resp).expect("Response is always JSON-serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mark_request() {
        let raw = r#"{"method":"mark","text":"hi","t":"00:01:23"}"#;
        let req = parse_request(raw).unwrap();
        assert_eq!(
            req,
            Request::Mark {
                text: "hi".into(),
                t: "00:01:23".into(),
            },
        );
    }

    #[test]
    fn parses_stop_request() {
        let raw = r#"{"method":"stop"}"#;
        let req = parse_request(raw).unwrap();
        assert_eq!(req, Request::Stop);
    }

    #[test]
    fn parses_status_request() {
        let raw = r#"{"method":"status"}"#;
        let req = parse_request(raw).unwrap();
        assert_eq!(req, Request::Status);
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_request("not-json").is_err());
        assert!(parse_request("{}").is_err());
        assert!(parse_request(r#"{"method":"unknown"}"#).is_err());
    }

    #[test]
    fn renders_ok_mark_ack() {
        let r = Response::Ok(OkPayload::MarkAck {
            appended: "[00:01:23] hi".into(),
        });
        let s = render_response(&r);
        assert!(s.contains("\"ok\":\"true\"") || s.contains("\"ok\":true"));
        assert!(s.contains("appended"));
    }

    #[test]
    fn renders_err_response() {
        let r = Response::Err { reason: "finalizing".into() };
        let s = render_response(&r);
        assert!(s.contains("\"reason\":\"finalizing\""));
    }

    #[test]
    fn round_trips_response_through_json() {
        let original = Response::Ok(OkPayload::Status {
            state: "capturing".into(),
            elapsed_seconds: 42,
        });
        let s = render_response(&original);
        let back: Response = serde_json::from_str(&s).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn handles_trailing_newline_in_request() {
        let raw = "{\"method\":\"stop\"}\n";
        assert_eq!(parse_request(raw).unwrap(), Request::Stop);
    }
}
