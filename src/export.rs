use std::{fs, path::Path};

use crate::transcription::Transcript;

pub fn write_json(transcript: &Transcript, path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(transcript)
        .map_err(|error| format!("JSONを生成できません: {error}"))?;
    fs::write(path, json).map_err(|error| format!("JSONを書き込めません: {error}"))
}

pub fn write_text(transcript: &Transcript, path: &Path) -> Result<(), String> {
    fs::write(path, transcript.to_text())
        .map_err(|error| format!("テキストを書き込めません: {error}"))
}
