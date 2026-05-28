use std::{fs::File, path::Path};

use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    errors::Error,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

use crate::transcription::TARGET_SAMPLE_RATE;

#[derive(Clone, Debug)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub original_sample_rate: u32,
}

pub fn load_audio_16k_mono(path: &Path) -> Result<AudioData, String> {
    let decoded = decode_mono(path)?;
    let samples = resample_linear(
        &decoded.samples,
        decoded.original_sample_rate,
        TARGET_SAMPLE_RATE,
    );

    Ok(AudioData {
        samples,
        original_sample_rate: decoded.original_sample_rate,
    })
}

fn decode_mono(path: &Path) -> Result<AudioData, String> {
    let file =
        Box::new(File::open(path).map_err(|error| format!("音声ファイルを開けません: {error}"))?);
    let source = MediaSourceStream::new(file, Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("対応していない音声形式です: {error}"))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| "音声トラックが見つかりません".to_owned())?;
    let track_id = track.id;

    let codec_params = track
        .codec_params
        .as_ref()
        .ok_or_else(|| "音声コーデック情報がありません".to_owned())?
        .audio()
        .ok_or_else(|| "音声コーデック情報がありません".to_owned())?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(|error| format!("音声デコーダーを作成できません: {error}"))?;

    let mut sample_rate = codec_params.sample_rate.unwrap_or(TARGET_SAMPLE_RATE);
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(Error::ResetRequired) => break,
            Err(error) => return Err(format!("音声パケットを読めません: {error}")),
        };

        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(buffer) => {
                sample_rate = buffer.spec().rate();
                let channel_count = buffer.spec().channels().count().max(1);
                let mut interleaved = vec![0.0_f32; buffer.samples_interleaved()];
                buffer.copy_to_slice_interleaved(&mut interleaved);

                for frame in interleaved.chunks(channel_count) {
                    let mono = frame.iter().copied().sum::<f32>() / frame.len() as f32;
                    samples.push(mono);
                }
            }
            Err(Error::DecodeError(_)) | Err(Error::IoError(_)) => continue,
            Err(error) => return Err(format!("音声デコードに失敗しました: {error}")),
        }
    }

    if samples.is_empty() {
        return Err("音声サンプルが読み込めませんでした".to_owned());
    }

    Ok(AudioData {
        samples,
        original_sample_rate: sample_rate,
    })
}

fn resample_linear(samples: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if samples.is_empty() || input_rate == output_rate {
        return samples.to_vec();
    }

    let output_len = ((samples.len() as u64 * output_rate as u64) / input_rate as u64) as usize;
    let mut output = Vec::with_capacity(output_len);
    let ratio = input_rate as f64 / output_rate as f64;

    for index in 0..output_len {
        let source_position = index as f64 * ratio;
        let left = source_position.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let fraction = (source_position - left as f64) as f32;
        output.push(samples[left] * (1.0 - fraction) + samples[right] * fraction);
    }

    output
}
