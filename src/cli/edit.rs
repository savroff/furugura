use crate::cli::list::scan_meetings;
use crate::config::Config;
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct EditArgs {
    /// Meeting id, partial-suffix, or shortlist offset (e.g., "1" = most recent).
    pub id: String,
}

pub async fn run(args: EditArgs, config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let output_root = config.output_dir_or(&paths);
    if !output_root.exists() {
        return Err(anyhow!("no meetings found at {}", output_root.display()));
    }

    let summaries = scan_meetings(&output_root, None)?;
    if summaries.is_empty() {
        return Err(anyhow!("no meetings found"));
    }

    let path = resolve_meeting_path(
        &args.id,
        &summaries
            .iter()
            .map(|s| (s.id.clone(), s.path.clone()))
            .collect::<Vec<_>>(),
    )?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| {
        if which("vi") {
            "vi".into()
        } else {
            "nano".into()
        }
    });

    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| {
            format!(
                "could not exec editor `{editor}` — set $EDITOR or install vi/nano",
            )
        })?;
    if !status.success() {
        return Err(anyhow!("editor `{editor}` exited {status}"));
    }
    Ok(())
}

fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve a CLI id-spec to a meeting.md path. Spec is one of:
///   - exact id (`2026-05-02-1430-team-standup`)
///   - partial suffix (`team-standup`, `1430-team-standup`)
///   - shortlist offset, 1-indexed against newest-first sort (`1`, `2`)
pub fn resolve_meeting_path(
    spec: &str,
    summaries: &[(String, PathBuf)],
) -> Result<PathBuf> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(anyhow!("empty id spec"));
    }

    // Numeric → shortlist offset.
    if let Ok(n) = spec.parse::<usize>() {
        if n == 0 || n > summaries.len() {
            return Err(anyhow!(
                "shortlist offset out of range: 1..={}",
                summaries.len(),
            ));
        }
        return Ok(summaries[n - 1].1.clone());
    }

    // Exact match wins.
    if let Some((_, p)) = summaries.iter().find(|(id, _)| id == spec) {
        return Ok(p.clone());
    }

    // Partial-suffix match.
    let candidates: Vec<&(String, PathBuf)> = summaries
        .iter()
        .filter(|(id, _)| id.ends_with(spec))
        .collect();
    match candidates.len() {
        0 => Err(anyhow!("no meeting matched `{spec}`")),
        1 => Ok(candidates[0].1.clone()),
        _ => {
            let names: Vec<String> = candidates.iter().map(|c| c.0.clone()).collect();
            Err(anyhow!(
                "`{spec}` matched {} meetings: {} — please be more specific",
                candidates.len(),
                names.join(", "),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> Vec<(String, PathBuf)> {
        vec![
            ("2026-05-03-1000-newer".to_string(), PathBuf::from("/x/2026-05-03-1000-newer/meeting.md")),
            ("2026-05-02-1430-team-standup".to_string(), PathBuf::from("/x/2026-05-02-1430-team-standup/meeting.md")),
            ("2026-05-01-0900-oldest".to_string(), PathBuf::from("/x/2026-05-01-0900-oldest/meeting.md")),
        ]
    }

    #[test]
    fn exact_match_wins() {
        let p = resolve_meeting_path("2026-05-02-1430-team-standup", &fixtures()).unwrap();
        assert!(p.ends_with("2026-05-02-1430-team-standup/meeting.md"));
    }

    #[test]
    fn shortlist_offset_resolves_to_newest_first() {
        let p = resolve_meeting_path("1", &fixtures()).unwrap();
        assert!(p.ends_with("2026-05-03-1000-newer/meeting.md"));
        let p3 = resolve_meeting_path("3", &fixtures()).unwrap();
        assert!(p3.ends_with("2026-05-01-0900-oldest/meeting.md"));
    }

    #[test]
    fn shortlist_out_of_range_errors() {
        let err = resolve_meeting_path("99", &fixtures()).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("out of range"));
    }

    #[test]
    fn shortlist_zero_errors() {
        let err = resolve_meeting_path("0", &fixtures()).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("out of range"));
    }

    #[test]
    fn partial_suffix_unique_match() {
        let p = resolve_meeting_path("team-standup", &fixtures()).unwrap();
        assert!(p.ends_with("2026-05-02-1430-team-standup/meeting.md"));
    }

    #[test]
    fn partial_suffix_ambiguous_errors() {
        let mut f = fixtures();
        f.push((
            "2026-05-04-0900-team-standup".to_string(),
            PathBuf::from("/x/2026-05-04-0900-team-standup/meeting.md"),
        ));
        let err = resolve_meeting_path("team-standup", &f).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("matched 2 meetings"));
    }

    #[test]
    fn unknown_spec_errors() {
        let err = resolve_meeting_path("nonsense", &fixtures()).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("no meeting matched"));
    }
}
