//! Align whisper transcript segments with pyannote RTTM segments by
//! timestamp overlap, producing speaker-labeled `LiveSegment`s.
//!
//! The merge is "for each whisper segment, pick the RTTM segment with
//! the maximum time overlap." If no RTTM segment overlaps, the segment
//! is left with `speaker = None` (caller decides whether to drop or
//! placeholder-label).

use super::jsonl::LiveSegment;
use super::rttm::RttmSegment;

/// Given whisper-derived segments and pyannote RTTM segments, return a
/// new vector of segments with `speaker` populated.
///
/// Tie-breaking: first RTTM segment encountered with the max overlap wins.
pub fn merge_speaker_labels(
    transcript: &[LiveSegment],
    diarization: &[RttmSegment],
) -> Vec<LiveSegment> {
    transcript
        .iter()
        .map(|seg| {
            let speaker = pick_best_speaker(seg, diarization);
            LiveSegment { speaker, ..seg.clone() }
        })
        .collect()
}

fn pick_best_speaker(seg: &LiveSegment, diarization: &[RttmSegment]) -> Option<String> {
    let mut best: Option<(f64, &str)> = None;
    for d in diarization {
        let overlap = overlap_seconds(seg.start, seg.end, d.start, d.end());
        if overlap <= 0.0 {
            continue;
        }
        match best {
            None => best = Some((overlap, &d.speaker)),
            Some((cur, _)) if overlap > cur => best = Some((overlap, &d.speaker)),
            _ => {}
        }
    }
    best.map(|(_, name)| name.to_string())
}

/// Inclusive-exclusive overlap of two intervals, clamped to non-negative.
fn overlap_seconds(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> f64 {
    let lo = a_start.max(b_start);
    let hi = a_end.min(b_end);
    (hi - lo).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, text: &str) -> LiveSegment {
        LiveSegment {
            t: super::super::jsonl::format_timestamp(start),
            speaker: None,
            text: text.into(),
            start,
            end,
        }
    }

    fn rttm(start: f64, dur: f64, speaker: &str) -> RttmSegment {
        RttmSegment {
            start,
            duration: dur,
            speaker: speaker.into(),
        }
    }

    #[test]
    fn assigns_speaker_with_full_overlap() {
        let t = vec![seg(1.0, 2.0, "hi")];
        let d = vec![rttm(0.0, 5.0, "SPEAKER_00")];
        let merged = merge_speaker_labels(&t, &d);
        assert_eq!(merged[0].speaker.as_deref(), Some("SPEAKER_00"));
    }

    #[test]
    fn picks_speaker_with_more_overlap() {
        let t = vec![seg(0.0, 2.0, "hi")];
        let d = vec![
            rttm(0.0, 0.4, "SPEAKER_00"), // 0.4s overlap
            rttm(0.4, 1.6, "SPEAKER_01"), // 1.6s overlap
        ];
        let merged = merge_speaker_labels(&t, &d);
        assert_eq!(merged[0].speaker.as_deref(), Some("SPEAKER_01"));
    }

    #[test]
    fn no_overlap_leaves_speaker_none() {
        let t = vec![seg(10.0, 11.0, "hi")];
        let d = vec![rttm(0.0, 1.0, "SPEAKER_00")];
        let merged = merge_speaker_labels(&t, &d);
        assert_eq!(merged[0].speaker, None);
    }

    #[test]
    fn empty_diarization_leaves_all_none() {
        let t = vec![seg(0.0, 1.0, "a"), seg(1.0, 2.0, "b")];
        let merged = merge_speaker_labels(&t, &[]);
        assert!(merged.iter().all(|s| s.speaker.is_none()));
    }

    #[test]
    fn tie_resolves_to_first_encountered() {
        let t = vec![seg(0.0, 2.0, "hi")];
        let d = vec![
            rttm(0.0, 1.0, "SPEAKER_00"),
            rttm(1.0, 1.0, "SPEAKER_01"),
        ];
        let merged = merge_speaker_labels(&t, &d);
        assert_eq!(merged[0].speaker.as_deref(), Some("SPEAKER_00"));
    }

    #[test]
    fn preserves_text_and_timing() {
        let t = vec![seg(1.0, 2.0, "hello world")];
        let d = vec![rttm(0.0, 5.0, "SPEAKER_42")];
        let merged = merge_speaker_labels(&t, &d);
        assert_eq!(merged[0].text, "hello world");
        assert_eq!(merged[0].start, 1.0);
        assert_eq!(merged[0].end, 2.0);
        assert_eq!(merged[0].speaker.as_deref(), Some("SPEAKER_42"));
    }

    #[test]
    fn overlap_helper_is_clamped_nonneg() {
        assert_eq!(overlap_seconds(0.0, 1.0, 2.0, 3.0), 0.0);
        assert_eq!(overlap_seconds(0.0, 2.0, 1.0, 3.0), 1.0);
        assert_eq!(overlap_seconds(0.0, 5.0, 1.0, 4.0), 3.0);
    }
}
