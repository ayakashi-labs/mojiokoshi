use std::path::Path;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
    WhisperVadParams,
};

#[derive(Clone, Debug)]
pub struct RawTranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub struct WhisperTranscriber {
    state: WhisperState,
    _context: WhisperContext,
    vad_model_path: String,
}

impl WhisperTranscriber {
    pub fn new(model_path: &Path, vad_model_path: &Path) -> Result<Self, String> {
        let params = WhisperContextParameters::default();
        let context = WhisperContext::new_with_params(model_path, params)
            .map_err(|error| format!("Whisperモデルを読み込めません: {error}"))?;
        let state = context
            .create_state()
            .map_err(|error| format!("Whisper stateを作成できません: {error}"))?;
        let vad_model_path = vad_model_path
            .to_str()
            .ok_or_else(|| "Whisper VADモデルのパスをUTF-8として扱えません".to_owned())?
            .to_owned();

        Ok(Self {
            state,
            _context: context,
            vad_model_path,
        })
    }

    pub fn transcribe(
        &mut self,
        samples: &[f32],
        offset_ms: u64,
    ) -> Result<Vec<RawTranscriptSegment>, String> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });

        let threads = std::thread::available_parallelism()
            .map(|count| count.get().min(8) as i32)
            .unwrap_or(4);
        params.set_n_threads(threads);
        params.set_language(Some("ja"));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_vad_model_path(Some(&self.vad_model_path));
        params.set_vad_params(WhisperVadParams::new());
        params.enable_vad(true);

        self.state
            .full(params, samples)
            .map_err(|error| format!("Whisper文字起こしに失敗しました: {error}"))?;

        Ok(self
            .state
            .as_iter()
            .map(|segment| RawTranscriptSegment {
                start_ms: offset_ms + timestamp_to_ms(segment.start_timestamp()),
                end_ms: offset_ms + timestamp_to_ms(segment.end_timestamp()),
                text: segment.to_string().trim().to_owned(),
            })
            .collect())
    }
}

fn timestamp_to_ms(timestamp: i64) -> u64 {
    timestamp.max(0) as u64 * 10
}
