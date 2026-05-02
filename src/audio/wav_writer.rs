//! Streaming WAV writer wrapping `hound`.
//!
//! Always writes 16-bit PCM stereo. The plan (U2) commits to 48 kHz; the
//! sample rate is taken from the caller so tests and v1.x changes don't have
//! to special-case the writer.

use anyhow::{Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub struct StereoWavWriter {
    inner: WavWriter<BufWriter<File>>,
    samples_written: u64,
}

impl StereoWavWriter {
    pub fn create(path: &Path, sample_rate: u32) -> Result<Self> {
        let spec = WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let inner = WavWriter::create(path, spec)
            .with_context(|| format!("could not open WAV for write: {}", path.display()))?;
        Ok(Self {
            inner,
            samples_written: 0,
        })
    }

    /// Write one stereo frame (left, right).
    pub fn write_frame(&mut self, left: i16, right: i16) -> Result<()> {
        self.inner.write_sample(left)?;
        self.inner.write_sample(right)?;
        self.samples_written += 1;
        Ok(())
    }

    /// Write a slice of interleaved stereo samples (`[L0, R0, L1, R1, ...]`).
    /// Length must be even.
    pub fn write_interleaved(&mut self, interleaved: &[i16]) -> Result<()> {
        debug_assert!(interleaved.len() % 2 == 0);
        for chunk in interleaved.chunks_exact(2) {
            self.inner.write_sample(chunk[0])?;
            self.inner.write_sample(chunk[1])?;
            self.samples_written += 1;
        }
        Ok(())
    }

    /// Number of stereo *frames* written so far (one frame = L+R pair).
    pub fn frames_written(&self) -> u64 {
        self.samples_written
    }

    pub fn finalize(self) -> Result<()> {
        self.inner
            .finalize()
            .context("could not finalize WAV: header rewrite failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_stereo_wav_with_correct_byte_count() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let mut w = StereoWavWriter::create(&path, 48_000).unwrap();
        // 1 second of silence at 48 kHz stereo s16 = 192,000 bytes of PCM data.
        for _ in 0..48_000 {
            w.write_frame(0, 0).unwrap();
        }
        assert_eq!(w.frames_written(), 48_000);
        w.finalize().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        // 44-byte canonical PCM RIFF header + 192,000 bytes of data.
        assert_eq!(bytes.len(), 44 + 192_000, "WAV file should be header + 1s PCM");

        // Roundtrip: read back via hound and confirm stereo / 48 kHz / s16.
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 2);
        assert_eq!(reader.spec().sample_rate, 48_000);
        assert_eq!(reader.spec().bits_per_sample, 16);
    }

    #[test]
    fn write_interleaved_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let mut w = StereoWavWriter::create(&path, 48_000).unwrap();
        let interleaved = vec![1i16, -1, 2, -2, 3, -3, 4, -4];
        w.write_interleaved(&interleaved).unwrap();
        assert_eq!(w.frames_written(), 4);
        w.finalize().unwrap();

        let mut reader = hound::WavReader::open(&path).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(samples, interleaved);
    }
}
