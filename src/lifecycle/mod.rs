//! Capture lifecycle: lockfile + state machine + UDS protocol.
//!
//! `furu start` acquires the lock, writes `active-meeting.json`, binds the
//! UDS socket, drives the capture + streaming pipeline, and transitions
//! through `capturing → finalizing → done` on `furu stop`. Other commands
//! (`furu mark`, `furu stop`, `furu live`, `furu cleanup`, `furu finalize`)
//! discover the active meeting via the lockfile and talk to the
//! orchestrator over UDS where applicable.

pub mod id;
pub mod lockfile;
pub mod orchestrator;
pub mod uds;

pub use orchestrator::{StartOptions, start_and_run};

pub use id::{make_id, slugify_title};
pub use lockfile::{
    ActiveMeeting, LifecycleState, LockGuard, acquire_lock, read_active_meeting,
    transition_state, write_active_meeting,
};
pub use uds::{Request, Response, parse_request, render_response};
