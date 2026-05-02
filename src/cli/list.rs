use crate::config::Config;
use crate::paths::Paths;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local};
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Show only meetings within the last <duration> (e.g., 7d, 24h, 30m).
    #[arg(long)]
    pub since: Option<String>,

    /// Maximum number of meetings to show.
    #[arg(long, default_value_t = 100)]
    pub limit: usize,

    /// Show all meetings (overrides --limit).
    #[arg(long, conflicts_with = "limit")]
    pub all: bool,
}

#[derive(Debug, Clone)]
pub struct MeetingSummary {
    pub id: String,
    pub path: PathBuf,
    pub start_time: Option<DateTime<Local>>,
    pub duration_minutes: Option<i64>,
    pub attendees: Vec<String>,
}

pub async fn run(args: ListArgs, config: &Config) -> Result<()> {
    let paths = Paths::discover()?;
    let output_root = config.output_dir_or(&paths);

    if !output_root.exists() {
        println!("no meetings found at {}", output_root.display());
        return Ok(());
    }

    let cutoff = match args.since.as_deref() {
        Some(s) => Some(parse_since(s)?),
        None => None,
    };

    let summaries = scan_meetings(&output_root, cutoff)?;
    if summaries.is_empty() {
        println!("no meetings found");
        return Ok(());
    }

    let limit = if args.all { summaries.len() } else { args.limit };
    let to_show = &summaries[..summaries.len().min(limit)];

    println!("{:<32}  {:<10}  {:>4}  {:>5}", "id", "date", "min", "att");
    for s in to_show {
        let date = s
            .start_time
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".to_string());
        let dur = s
            .duration_minutes
            .map(|d| d.to_string())
            .unwrap_or_else(|| "—".to_string());
        println!(
            "{:<32}  {:<10}  {:>4}  {:>5}",
            truncate(&s.id, 32),
            date,
            dur,
            s.attendees.len(),
        );
    }
    if summaries.len() > to_show.len() {
        println!(
            "({} more — use --all or --limit N)",
            summaries.len() - to_show.len(),
        );
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}

/// Glob `<output_root>/<id>/meeting.md`, parse frontmatter, filter by
/// `cutoff` (only when set). Returns sorted newest-first.
pub fn scan_meetings(
    output_root: &Path,
    cutoff: Option<DateTime<Local>>,
) -> Result<Vec<MeetingSummary>> {
    let mut summaries = Vec::new();
    let dir_iter = std::fs::read_dir(output_root)
        .with_context(|| format!("could not read {}", output_root.display()))?;
    for entry in dir_iter {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id_from_dir = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Cheap pre-filter: parse the date prefix from the directory name
        // before opening the file, to skip ancient history when --since is set.
        if let Some(c) = cutoff
            && let Some(dt) = parse_id_date(&id_from_dir)
            && dt < c
        {
            continue;
        }
        let md_path = path.join("meeting.md");
        if !md_path.exists() {
            continue;
        }
        match parse_meeting_md(&md_path) {
            Ok(s) => summaries.push(s),
            Err(_) => continue,
        }
    }
    summaries.sort_by_key(|s| std::cmp::Reverse(s.start_time));
    Ok(summaries)
}

/// Parse a meeting.md file's frontmatter into a summary. Returns an
/// error only on truly malformed input (no frontmatter).
fn parse_meeting_md(path: &Path) -> Result<MeetingSummary> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let frontmatter = extract_frontmatter(&raw)
        .ok_or_else(|| anyhow!("no frontmatter in {}", path.display()))?;
    let kvs = parse_frontmatter_lines(frontmatter);

    let id = kvs
        .iter()
        .find_map(|(k, v)| (k == "id").then(|| v.clone()))
        .or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    let start_time = kvs
        .iter()
        .find(|(k, _)| k == "start_time")
        .map(|(_, v)| v.as_str())
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|dt| dt.with_timezone(&Local));

    let duration_minutes = kvs
        .iter()
        .find(|(k, _)| k == "duration_minutes")
        .map(|(_, v)| v.as_str())
        .and_then(|v| v.parse::<i64>().ok());

    let attendees = kvs
        .iter()
        .find(|(k, _)| k == "attendees")
        .map(|(_, v)| parse_attendees(v))
        .unwrap_or_default();

    Ok(MeetingSummary {
        id,
        path: path.to_path_buf(),
        start_time,
        duration_minutes,
        attendees,
    })
}

/// Return the frontmatter block (without the `---` fences) if present.
pub fn extract_frontmatter(raw: &str) -> Option<&str> {
    let r = raw.strip_prefix("---\n")?;
    let end = r.find("\n---\n")?;
    Some(&r[..end])
}

pub fn parse_frontmatter_lines(block: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in block.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let mut val = v.trim().to_string();
            if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
                val = val[1..val.len() - 1].replace("\\\"", "\"");
            }
            out.push((key, val));
        }
    }
    out
}

/// Parse `[a, b, c]` or `[]` into a list. Strips surrounding quotes.
pub fn parse_attendees(value: &str) -> Vec<String> {
    let v = value.trim();
    if !v.starts_with('[') || !v.ends_with(']') {
        return Vec::new();
    }
    let inner = &v[1..v.len() - 1];
    if inner.trim().is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                s[1..s.len() - 1].replace("\\\"", "\"")
            } else {
                s.to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse the `--since` argument: `<n><unit>` where unit is m, h, d, w.
pub fn parse_since(s: &str) -> Result<DateTime<Local>> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow!("empty --since"));
    }
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: i64 = num
        .parse()
        .with_context(|| format!("invalid --since duration: `{s}`"))?;
    let dur = match unit {
        "m" => chrono::Duration::minutes(n),
        "h" => chrono::Duration::hours(n),
        "d" => chrono::Duration::days(n),
        "w" => chrono::Duration::weeks(n),
        _ => {
            return Err(anyhow!(
                "unknown duration unit `{unit}` (want m, h, d, w)",
            ));
        }
    };
    Ok(Local::now() - dur)
}

/// Extract a `Local` datetime from an id of shape `YYYY-MM-DD-HHMM[-...]`.
pub fn parse_id_date(id: &str) -> Option<DateTime<Local>> {
    let parts: Vec<&str> = id.splitn(5, '-').collect();
    if parts.len() < 4 {
        return None;
    }
    let date = format!("{}-{}-{}", parts[0], parts[1], parts[2]);
    let time = parts[3];
    if time.len() != 4 {
        return None;
    }
    let h: u32 = time[..2].parse().ok()?;
    let m: u32 = time[2..].parse().ok()?;
    let (y, mo, d): (i32, u32, u32) = {
        let mut iter = date.split('-');
        let y = iter.next()?.parse().ok()?;
        let mo = iter.next()?.parse().ok()?;
        let d = iter.next()?.parse().ok()?;
        (y, mo, d)
    };
    use chrono::TimeZone;
    Local.with_ymd_and_hms(y, mo, d, h, m, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extracts_frontmatter_block() {
        let raw = "---\nid: 1\nfoo: bar\n---\n\n# Title\n";
        let fm = extract_frontmatter(raw).unwrap();
        assert_eq!(fm, "id: 1\nfoo: bar");
    }

    #[test]
    fn missing_frontmatter_returns_none() {
        assert!(extract_frontmatter("# Title\n").is_none());
        assert!(extract_frontmatter("---\nno close").is_none());
    }

    #[test]
    fn parses_kv_lines_with_quoting() {
        let block = "id: 2026-05-02-1430-team\nfoo: \"with: colon\"\nempty: \"\"";
        let kvs = parse_frontmatter_lines(block);
        assert_eq!(kvs[0], ("id".to_string(), "2026-05-02-1430-team".to_string()));
        assert_eq!(kvs[1], ("foo".to_string(), "with: colon".to_string()));
        assert_eq!(kvs[2], ("empty".to_string(), "".to_string()));
    }

    #[test]
    fn parses_attendees_list() {
        assert_eq!(parse_attendees("[]"), Vec::<String>::new());
        assert_eq!(parse_attendees("[alice@x.com]"), vec!["alice@x.com"]);
        assert_eq!(
            parse_attendees("[\"alice@x.com\", \"bob@y.com\"]"),
            vec!["alice@x.com", "bob@y.com"],
        );
    }

    #[test]
    fn parse_since_units() {
        let m = parse_since("30m").unwrap();
        assert!(m < Local::now());
        let h = parse_since("24h").unwrap();
        assert!(h < Local::now());
        let d = parse_since("7d").unwrap();
        assert!(d < Local::now());
        let w = parse_since("2w").unwrap();
        assert!(w < Local::now());
        // 7d cutoff is more recent than 2w cutoff.
        assert!(d > w);
    }

    #[test]
    fn parse_since_rejects_invalid() {
        assert!(parse_since("").is_err());
        assert!(parse_since("abc").is_err());
        assert!(parse_since("7y").is_err());
    }

    #[test]
    fn parse_id_date_extracts_correctly() {
        let dt = parse_id_date("2026-05-02-1430-team-standup").unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M").to_string(), "2026-05-02 14:30");
    }

    #[test]
    fn parse_id_date_handles_no_slug() {
        let dt = parse_id_date("2026-05-02-1430").unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M").to_string(), "2026-05-02 14:30");
    }

    #[test]
    fn parse_id_date_rejects_garbage() {
        assert!(parse_id_date("not-an-id").is_none());
        assert!(parse_id_date("2026-05-02").is_none());
        assert!(parse_id_date("2026-13-99-2599").is_none());
    }

    #[test]
    fn scan_meetings_sorts_newest_first() {
        let dir = tempdir().unwrap();
        for (id, ts) in [
            ("2026-05-02-1430-old", "2026-05-02T14:30:00-04:00"),
            ("2026-05-03-1000-newer", "2026-05-03T10:00:00-04:00"),
            ("2026-05-01-0900-oldest", "2026-05-01T09:00:00-04:00"),
        ] {
            let mdir = dir.path().join(id);
            std::fs::create_dir(&mdir).unwrap();
            std::fs::write(
                mdir.join("meeting.md"),
                format!(
                    "---\nfurugura_version: 1\nid: {id}\nstart_time: {ts}\nduration_minutes: 30\nattendees: []\n---\n\n# X\n",
                ),
            )
            .unwrap();
        }
        let s = scan_meetings(dir.path(), None).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].id, "2026-05-03-1000-newer");
        assert_eq!(s[1].id, "2026-05-02-1430-old");
        assert_eq!(s[2].id, "2026-05-01-0900-oldest");
    }

    #[test]
    fn scan_meetings_with_since_filters() {
        let dir = tempdir().unwrap();
        for id in ["2020-01-01-0900-ancient", "2026-05-03-1000-recent"] {
            let mdir = dir.path().join(id);
            std::fs::create_dir(&mdir).unwrap();
            std::fs::write(
                mdir.join("meeting.md"),
                format!(
                    "---\nid: {id}\nstart_time: 2020-01-01T09:00:00-04:00\nduration_minutes: 30\nattendees: []\n---\n",
                ),
            )
            .unwrap();
        }
        // Cut off 1 day ago — both files have ancient start_time but only
        // the directory whose date prefix is recent enough survives the
        // pre-filter (the rest aren't even opened).
        let cutoff = Local::now() - chrono::Duration::days(1);
        let s = scan_meetings(dir.path(), Some(cutoff)).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "2026-05-03-1000-recent");
    }

    #[test]
    fn scan_meetings_skips_dirs_without_meeting_md() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("2026-05-02-1430-empty")).unwrap();
        let s = scan_meetings(dir.path(), None).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn truncate_keeps_short_strings() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("longerthan", 5), "long…");
    }
}
