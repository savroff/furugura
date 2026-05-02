//! Resolve the PipeWire monitor source corresponding to the user's
//! default sink, plus a coarse classification of the sink type for
//! the speaker-bleed warning at `furu start`.
//!
//! The parsing logic is separated from `pactl` invocation so it can be
//! unit-tested against captured fixtures.

use anyhow::{Context, Result, anyhow};
use tokio::process::Command;

/// Coarse classification of the default audio sink, used for the
/// "speaker bleed risk" warning when no headphones are plugged in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkClass {
    /// Headphones, headset, or external monitor → channel-split prior is safe.
    Headphones,
    /// Built-in laptop speakers → mic may capture remote audio.
    LaptopSpeakers,
    /// Bluetooth output → channel-split prior is safe (BT headset).
    Bluetooth,
    /// Unknown / unclassified.
    Unknown,
}

impl SinkClass {
    pub fn risks_speaker_bleed(self) -> bool {
        matches!(self, SinkClass::LaptopSpeakers)
    }
}

/// Resolve the monitor source name corresponding to `default_sink`.
/// Returns the first source whose name matches `<default_sink>.monitor`,
/// or any `.monitor`-suffixed source as a fallback if no exact match.
pub fn pick_monitor_for_sink(
    sources_short_output: &str,
    default_sink: &str,
) -> Option<String> {
    let expected = format!("{default_sink}.monitor");
    let mut fallback: Option<String> = None;
    for line in sources_short_output.lines() {
        // Format: `<id>\t<name>\t<driver>\t<spec>\t<state>`
        let name = match line.split('\t').nth(1) {
            Some(n) => n.trim(),
            None => continue,
        };
        if name == expected {
            return Some(name.to_string());
        }
        if fallback.is_none() && name.ends_with(".monitor") {
            fallback = Some(name.to_string());
        }
    }
    fallback
}

/// Parse the default sink name out of `pactl info` output.
pub fn parse_default_sink(pactl_info_output: &str) -> Option<String> {
    for line in pactl_info_output.lines() {
        if let Some(rest) = line.strip_prefix("Default Sink:") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Classify a sink name into one of the broad categories used for the
/// speaker-bleed warning. We only match on substrings of the sink name
/// that are stable across PipeWire / PulseAudio versions; anything else
/// becomes `Unknown`.
pub fn classify_sink(sink_name: &str) -> SinkClass {
    let n = sink_name.to_ascii_lowercase();
    if n.contains("bluez") || n.contains("bluetooth") {
        return SinkClass::Bluetooth;
    }
    if n.contains("headphone") || n.contains("headset") || n.contains("hdmi") {
        return SinkClass::Headphones;
    }
    if n.contains("speaker") || n.contains("analog-output") {
        return SinkClass::LaptopSpeakers;
    }
    SinkClass::Unknown
}

/// Run `pactl info` and extract the default sink name.
pub async fn default_sink_name() -> Result<String> {
    let out = Command::new("pactl")
        .arg("info")
        .output()
        .await
        .context("could not run `pactl info` (is pactl on $PATH?)")?;
    if !out.status.success() {
        return Err(anyhow!(
            "pactl info exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_default_sink(&stdout).ok_or_else(|| {
        anyhow!("pactl info did not report a Default Sink — is the audio server running?")
    })
}

/// Resolve the monitor source for the user's default sink.
/// Returns the source name suitable for `pw-record --target ...`.
pub async fn default_monitor_source() -> Result<String> {
    let sink = default_sink_name().await?;
    let out = Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
        .await
        .context("could not run `pactl list short sources`")?;
    if !out.status.success() {
        return Err(anyhow!(
            "pactl list short sources exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    pick_monitor_for_sink(&stdout, &sink).ok_or_else(|| {
        anyhow!(
            "no monitor source found for default sink '{sink}' \
             — meeting capture cannot record system audio",
        )
    })
}

/// Classify the user's current default sink (mic-bleed risk warning).
pub async fn default_sink_class() -> Result<SinkClass> {
    let sink = default_sink_name().await?;
    Ok(classify_sink(&sink))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_exact_monitor_match() {
        let pactl_short = "\
40\talsa_input.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts16le 2ch 48000Hz\tIDLE
44\talsa_output.pci-0000_00_1f.3.analog-stereo.monitor\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED
50\talsa_output.usb-Generic_USB_Audio.monitor\tPipeWire\ts16le 2ch 48000Hz\tIDLE
";
        let sink = "alsa_output.pci-0000_00_1f.3.analog-stereo";
        assert_eq!(
            pick_monitor_for_sink(pactl_short, sink).as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"),
        );
    }

    #[test]
    fn falls_back_to_any_monitor_if_no_exact() {
        let pactl_short = "\
50\talsa_output.usb-Generic_USB_Audio.monitor\tPipeWire\ts16le 2ch 48000Hz\tIDLE
";
        let sink = "some_other_sink_that_does_not_match";
        let picked = pick_monitor_for_sink(pactl_short, sink).unwrap();
        assert!(picked.ends_with(".monitor"));
    }

    #[test]
    fn returns_none_when_no_monitor_present() {
        let pactl_short = "\
40\talsa_input.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts16le 2ch 48000Hz\tIDLE
";
        assert_eq!(
            pick_monitor_for_sink(pactl_short, "anything"),
            None,
        );
    }

    #[test]
    fn parses_default_sink_from_pactl_info() {
        let info = "\
Server String: /run/user/1000/pulse/native
Library Protocol Version: 35
Server Protocol Version: 35
Server Name: PulseAudio (on PipeWire 1.6.4)
Default Sink: alsa_output.pci-0000_00_1f.3.analog-stereo
Default Source: alsa_input.pci-0000_00_1f.3.analog-stereo
Cookie: 0001:0002
";
        assert_eq!(
            parse_default_sink(info).as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo"),
        );
    }

    #[test]
    fn parses_default_sink_returns_none_when_absent() {
        let info = "Server Name: foo\nLibrary Protocol Version: 35\n";
        assert_eq!(parse_default_sink(info), None);
    }

    #[test]
    fn classify_known_sinks() {
        assert_eq!(
            classify_sink("bluez_output.AA_BB_CC_DD_EE_FF.1"),
            SinkClass::Bluetooth,
        );
        assert_eq!(
            classify_sink("alsa_output.pci-0000_00_1f.3.analog-output-speaker"),
            SinkClass::LaptopSpeakers,
        );
        assert_eq!(
            classify_sink("alsa_output.usb-Sennheiser_HD-headphones-stereo"),
            SinkClass::Headphones,
        );
        assert_eq!(
            classify_sink("alsa_output.pci-0000_00_1f.3.hdmi-stereo"),
            SinkClass::Headphones,
        );
        assert_eq!(classify_sink("something-weird"), SinkClass::Unknown);
    }

    #[test]
    fn risks_speaker_bleed_only_for_laptop_speakers() {
        assert!(SinkClass::LaptopSpeakers.risks_speaker_bleed());
        assert!(!SinkClass::Bluetooth.risks_speaker_bleed());
        assert!(!SinkClass::Headphones.risks_speaker_bleed());
        assert!(!SinkClass::Unknown.risks_speaker_bleed());
    }
}
