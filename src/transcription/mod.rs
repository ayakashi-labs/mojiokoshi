pub mod whisper;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::audio;

use whisper::WhisperTranscriber;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transcript {
    pub source_path: String,
    pub model_path: String,
    pub vad_model_path: String,
    pub original_sample_rate: u32,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub fn transcribe_file(
    audio_path: &Path,
    model_path: &Path,
    vad_model_path: &Path,
    mut emit: impl FnMut(&str, f32),
) -> Result<Transcript, String> {
    emit("音声を読み込んでいます", 0.05);
    let audio = audio::load_audio_16k_mono(audio_path)?;

    emit("Whisperモデルを読み込んでいます", 0.20);
    let mut transcriber = WhisperTranscriber::new(model_path, vad_model_path)?;

    let mut segments = Vec::new();

    emit("Whisperで文字起こししています", 0.35);
    let raw_segments = transcriber.transcribe(&audio.samples, 0)?;

    for raw_segment in raw_segments {
        if raw_segment.text.trim().is_empty() {
            continue;
        }

        let mut segment = TranscriptSegment {
            index: segments.len() + 1,
            start_ms: raw_segment.start_ms,
            end_ms: raw_segment.end_ms,
            text: raw_segment.text,
        };
        if segment.end_ms <= segment.start_ms {
            segment.end_ms = segment.start_ms + 10;
        }

        segments.push(segment);
    }

    emit("結果を整えています", 0.98);
    segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
    for (index, segment) in segments.iter_mut().enumerate() {
        segment.index = index + 1;
    }

    Ok(Transcript {
        source_path: audio_path.to_string_lossy().into_owned(),
        model_path: model_path.to_string_lossy().into_owned(),
        vad_model_path: vad_model_path.to_string_lossy().into_owned(),
        original_sample_rate: audio.original_sample_rate,
        segments,
    })
}

impl Transcript {
    pub fn to_text(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("source: {}\n", self.source_path));
        output.push_str(&format!("model: {}\n", self.model_path));
        output.push_str(&format!("vad_model: {}\n\n", self.vad_model_path));

        for segment in &self.segments {
            output.push_str(&format!(
                "[{} - {}] {}\n",
                format_timestamp(segment.start_ms),
                format_timestamp(segment.end_ms),
                segment.text
            ));
        }

        output
    }
}

pub fn format_timestamp(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let milliseconds = ms % 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}")
    } else {
        format!("{minutes:02}:{seconds:02}.{milliseconds:03}")
    }
}
