//! Minimal RTTM read/write for pyannote output.
//!
//! RTTM (Rich Transcription Time Marked) is a space-delimited format. We
//! only emit `SPEAKER` rows; everything else is a no-op for our use case.
//!
//! Row format we generate:
//!   `SPEAKER <file-id> 1 <start> <duration> <NA> <NA> <speaker> <NA> <NA>`
//!
//! Reference: NIST RT evaluation specs.

use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct RttmSegment {
    pub start: f64,
    pub duration: f64,
    pub speaker: String,
}

impl RttmSegment {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

pub fn write_rttm(path: &Path, file_id: &str, segments: &[RttmSegment]) -> Result<()> {
    let mut f = File::create(path)
        .with_context(|| format!("could not open RTTM for write: {}", path.display()))?;
    for seg in segments {
        // Defensive formatting: 3 decimal places matches pyannote's default.
        writeln!(
            f,
            "SPEAKER {file_id} 1 {:.3} {:.3} <NA> <NA> {} <NA> <NA>",
            seg.start, seg.duration, seg.speaker,
        )?;
    }
    f.flush()?;
    Ok(())
}

pub fn parse_rttm(text: &str) -> Result<Vec<RttmSegment>> {
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.first().copied() != Some("SPEAKER") {
            continue;
        }
        // SPEAKER file-id chnl tbeg tdur ortho stype name conf slat
        // Indices: 0       1       2    3    4    5     6     7    8    9
        let start: f64 = cols.get(3).copied().unwrap_or("").parse().with_context(|| {
            format!("RTTM line {} has invalid start time", idx + 1)
        })?;
        let duration: f64 = cols.get(4).copied().unwrap_or("").parse().with_context(|| {
            format!("RTTM line {} has invalid duration", idx + 1)
        })?;
        let speaker = cols
            .get(7)
            .copied()
            .ok_or_else(|| anyhow!("RTTM line {} missing speaker column", idx + 1))?
            .to_string();
        out.push(RttmSegment { start, duration, speaker });
    }
    Ok(out)
}

pub fn read_rttm(path: &Path) -> Result<Vec<RttmSegment>> {
    let f = File::open(path)
        .with_context(|| format!("could not open RTTM: {}", path.display()))?;
    let mut text = String::new();
    let mut r = BufReader::new(f);
    use std::io::Read;
    r.read_to_string(&mut text)?;
    parse_rttm(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_parse_round_trips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.rttm");
        let segs = vec![
            RttmSegment { start: 0.0, duration: 1.250, speaker: "SPEAKER_00".into() },
            RttmSegment { start: 1.250, duration: 2.500, speaker: "SPEAKER_01".into() },
            RttmSegment { start: 3.750, duration: 0.500, speaker: "SPEAKER_00".into() },
        ];
        write_rttm(&path, "meeting", &segs).unwrap();
        let read = read_rttm(&path).unwrap();
        assert_eq!(read, segs);
    }

    #[test]
    fn parses_pyannote_style_rttm() {
        // Minimal pyannote sample
        let text = "\
SPEAKER meeting 1 0.030 1.520 <NA> <NA> SPEAKER_00 <NA> <NA>
SPEAKER meeting 1 1.550 0.770 <NA> <NA> SPEAKER_01 <NA> <NA>
";
        let segs = parse_rttm(text).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].speaker, "SPEAKER_00");
        assert_eq!(segs[0].start, 0.030);
        assert_eq!(segs[0].duration, 1.520);
        assert_eq!(segs[1].speaker, "SPEAKER_01");
    }

    #[test]
    fn ignores_non_speaker_rows_and_comments() {
        let text = "\
;; this is a comment
LEXEME meeting 1 0.0 0.5 hello <NA> <NA> <NA> <NA>
SPEAKER meeting 1 0.0 1.0 <NA> <NA> SPEAKER_00 <NA> <NA>
";
        let segs = parse_rttm(text).unwrap();
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn end_time_is_start_plus_duration() {
        let s = RttmSegment { start: 1.0, duration: 2.5, speaker: "X".into() };
        assert_eq!(s.end(), 3.5);
    }

    #[test]
    fn malformed_numeric_returns_error() {
        let text = "SPEAKER meeting 1 not-a-number 1.0 <NA> <NA> SPEAKER_00 <NA> <NA>";
        let err = parse_rttm(text).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("invalid start"), "{msg}");
    }
}
