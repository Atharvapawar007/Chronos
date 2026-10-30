//! Polyphonic Audio Synthesizer & Desktop Notification Dispatcher (`audio.rs`).
//!
//! Generates an embedded polyphonic singing-bowl / harmonic chime 16-bit PCM WAV file on demand
//! and plays it asynchronously via available platform players (`pw-play`, `paplay`, `aplay`, `afplay`),
//! falling back gracefully to a terminal bell (`\x07`) if no audio daemon is installed.

use crate::domain::{ActivityKind, NotificationPrefs};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::io::Write;
use std::process::{Child, Command, Stdio};

pub struct AudioNotifier {
    cached_wav_path: PathBuf,
}

impl Default for AudioNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioNotifier {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(env::temp_dir)
            .join("chronos");
        let cached_wav_path = cache_dir.join("chronos_harmonic_chime.wav");
        Self { cached_wav_path }
    }

    pub fn notify_phase_complete(&self, kind: ActivityKind, prefs: &NotificationPrefs) {
        if prefs.desktop_notifications {
            let title = format!("Chronos — {} Complete", kind.label());
            let body = match kind {
                ActivityKind::Focus => "Focus block completed. Claim your recovery break or record session notes.",
                ActivityKind::Reading => "Reading Sanctuary session completed. Log your ending page checkpoint.",
                ActivityKind::ShortBreak => "Short breather finished. Ready for your next focus block.",
                ActivityKind::LongBreak => "Long recovery break finished. Full cadence reset and ready.",
            };
            send_desktop_notification(&title, body);
        }

        if prefs.sound_enabled && prefs.sound_preset != "silent" {
            self.play_harmonic_chime();
        }
    }

    pub fn play_harmonic_chime(&self) {
        if !self.cached_wav_path.exists() {
            if let Some(parent) = self.cached_wav_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let wav_bytes = synthesize_singing_bowl_wav();
            let _ = fs::write(&self.cached_wav_path, wav_bytes);
        }

        let path_str = self.cached_wav_path.to_string_lossy().to_string();
        for (bin, args) in [
            ("pw-play", vec![path_str.as_str()]),
            ("paplay", vec![path_str.as_str()]),
            ("aplay", vec!["-q", path_str.as_str()]),
            ("afplay", vec![path_str.as_str()]),
        ] {
            if let Ok(child) = Command::new(bin)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                reap_in_background(child);
                return;
            }
        }
        // No audio player available: fall back to the terminal bell.
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x07");
        let _ = out.flush();
    }
}

/// Waits for a spawned helper on a throwaway thread so finished players never linger as zombies.
fn reap_in_background(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

fn send_desktop_notification(title: &str, body: &str) {
    if let Ok(child) = Command::new("notify-send")
        .args(["-a", "Chronos", title, body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        reap_in_background(child);
    }
}

/// Synthesizes a 1.4-second polyphonic harmonic chord (C5 + G5 + E6 singing-bowl partials)
/// as a standard 44.1 kHz 16-bit mono PCM WAV buffer.
pub fn synthesize_singing_bowl_wav() -> Vec<u8> {
    let sample_rate: u32 = 44_100;
    let duration_secs: f64 = 1.4;
    let num_samples: u32 = ((sample_rate as f64) * duration_secs) as u32;
    let data_size: u32 = num_samples * 2;
    let file_size: u32 = 36 + data_size;

    let mut buf = Vec::with_capacity((44 + data_size) as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // Mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    let freqs = [(523.25f64, 0.45f64), (783.99f64, 0.32f64), (1318.51f64, 0.23f64)];
    for i in 0..num_samples {
        let t = (i as f64) / (sample_rate as f64);
        let attack = (t / 0.015).min(1.0);
        let decay = (-t * 2.8).exp();
        let env = attack * decay;

        let mut sig = 0.0f64;
        for &(freq, amp) in &freqs {
            sig += (2.0 * std::f64::consts::PI * freq * t).sin() * amp;
        }
        let sample = (sig * env * 18_000.0).clamp(-32_767.0, 32_767.0) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}
