use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::Duration,
};

use eframe::egui;

use crate::{
    export,
    transcription::{self, Transcript, TranscriptSegment, format_timestamp},
};

const DEFAULT_MODEL_PATH: &str = "models/ggml-large-v3-turbo-q5_0.bin";
const DEFAULT_VAD_MODEL_PATH: &str = "models/ggml-silero-v6.2.0.bin";
const JAPANESE_FONT_NAME: &str = "windows_japanese";

enum WorkerEvent {
    Progress { status: String, progress: f32 },
    Finished(Result<Transcript, String>),
}

pub struct MeetingMinutesApp {
    selected_file: Option<PathBuf>,
    receiver: Option<Receiver<WorkerEvent>>,
    running: bool,
    progress: f32,
    status: String,
    error: Option<String>,
    transcript: Option<Transcript>,
}

impl MeetingMinutesApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_japanese_fonts(&cc.egui_ctx);

        Self {
            selected_file: None,
            receiver: None,
            running: false,
            progress: 0.0,
            status: "音声ファイルをドロップしてください".to_owned(),
            error: None,
            transcript: None,
        }
    }

    fn poll_worker(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };

        let mut finished = false;
        while let Ok(event) = receiver.try_recv() {
            match event {
                WorkerEvent::Progress { status, progress } => {
                    self.status = status;
                    self.progress = progress.clamp(0.0, 1.0);
                }
                WorkerEvent::Finished(result) => {
                    self.running = false;
                    self.progress = 1.0;
                    finished = true;
                    match result {
                        Ok(transcript) => {
                            self.status = "完了".to_owned();
                            self.error = None;
                            self.transcript = Some(transcript);
                        }
                        Err(error) => {
                            self.status = "失敗".to_owned();
                            self.error = Some(error);
                        }
                    }
                }
            }
        }

        if finished {
            self.receiver = None;
        }
    }

    fn start_transcription(&mut self) {
        let Some(audio_path) = self.selected_file.clone() else {
            self.error = Some("音声ファイルが選択されていません".to_owned());
            return;
        };

        let model_path = resolve_resource_path(DEFAULT_MODEL_PATH);
        if !model_path.exists() {
            self.error = Some(format!(
                "Whisperモデルが見つかりません: {}",
                model_path.display()
            ));
            return;
        }

        let vad_model_path = resolve_resource_path(DEFAULT_VAD_MODEL_PATH);
        if !vad_model_path.exists() {
            self.error = Some(format!(
                "Whisper VADモデルが見つかりません: {}",
                vad_model_path.display()
            ));
            return;
        }

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.running = true;
        self.progress = 0.0;
        self.error = None;
        self.transcript = None;
        self.status = "開始しています".to_owned();

        std::thread::spawn(move || {
            let result = transcription::transcribe_file(
                &audio_path,
                &model_path,
                &vad_model_path,
                |status, progress| {
                    let _ = sender.send(WorkerEvent::Progress {
                        status: status.to_owned(),
                        progress,
                    });
                },
            );

            let _ = sender.send(WorkerEvent::Finished(result));
        });
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());
        for file in dropped_files {
            if let Some(path) = file.path {
                if is_audio_file(&path) {
                    self.selected_file = Some(path);
                    self.status = "実行できます".to_owned();
                    self.error = None;
                    self.transcript = None;
                    self.progress = 0.0;
                    break;
                }
            }
        }
    }

    fn display_segments(&self) -> Vec<TranscriptSegment> {
        let mut segments = self
            .transcript
            .as_ref()
            .map(|transcript| transcript.segments.clone())
            .unwrap_or_default();

        segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));

        segments
    }

    fn export_json(&mut self) {
        let Some(transcript) = &self.transcript else {
            return;
        };

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("minutes.json")
            .save_file()
        {
            match export::write_json(transcript, &path) {
                Ok(()) => self.status = format!("JSONを書き出しました: {}", path.display()),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn export_text(&mut self) {
        let Some(transcript) = &self.transcript else {
            return;
        };

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name("minutes.txt")
            .save_file()
        {
            match export::write_text(transcript, &path) {
                Ok(()) => self.status = format!("テキストを書き出しました: {}", path.display()),
                Err(error) => self.error = Some(error),
            }
        }
    }
}

impl eframe::App for MeetingMinutesApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ui.ctx());
        self.poll_worker();

        if self.running {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("議事録文字起こし");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("音声ファイル");
                let file_name = self
                    .selected_file
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("未選択");
                ui.monospace(file_name);
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_run = self.selected_file.is_some() && !self.running;
                if ui.add_enabled(can_run, egui::Button::new("実行")).clicked() {
                    self.start_transcription();
                }

                ui.separator();

                let can_export = self.transcript.is_some() && !self.running;
                if ui
                    .add_enabled(can_export, egui::Button::new("JSON出力"))
                    .clicked()
                {
                    self.export_json();
                }
                if ui
                    .add_enabled(can_export, egui::Button::new("テキスト出力"))
                    .clicked()
                {
                    self.export_text();
                }
            });

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.set_min_height(86.0);
                ui.vertical_centered(|ui| {
                    ui.label("ここに音声ファイルをドラッグアンドドロップ");
                    ui.label("wav / mp3 / m4a / flac / ogg");
                });
            });

            ui.add_space(8.0);
            ui.add(egui::ProgressBar::new(self.progress).show_percentage());
            ui.label(&self.status);

            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(190, 40, 40), error);
            }

            if let Some(transcript) = &self.transcript {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(format!("セグメント: {}", transcript.segments.len()));
                });

                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("transcript_grid")
                        .striped(true)
                        .num_columns(3)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.strong("時刻");
                            ui.strong("終了");
                            ui.strong("内容");
                            ui.end_row();

                            for segment in self.display_segments() {
                                ui.monospace(format_timestamp(segment.start_ms));
                                ui.monospace(format_timestamp(segment.end_ms));
                                ui.label(segment.text);
                                ui.end_row();
                            }
                        });
                });
            }
        });
    }
}

fn is_audio_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase()),
        Some(extension)
            if matches!(
                extension.as_str(),
                "wav" | "mp3" | "m4a" | "mp4" | "aac" | "flac" | "ogg" | "opus" | "aiff"
            )
    )
}

fn resolve_resource_path(relative_path: &str) -> PathBuf {
    let working_dir_path = PathBuf::from(relative_path);
    if working_dir_path.exists() {
        return working_dir_path;
    }

    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        return exe_dir.join(relative_path);
    }

    working_dir_path
}

fn configure_japanese_fonts(ctx: &egui::Context) {
    let Some(font_bytes) = load_japanese_font() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        JAPANESE_FONT_NAME.to_owned(),
        Arc::new(egui::FontData::from_owned(font_bytes)),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, JAPANESE_FONT_NAME.to_owned());
    }

    ctx.set_fonts(fonts);
}

fn load_japanese_font() -> Option<Vec<u8>> {
    let fonts_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .map(|path| path.join("Fonts"))
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\Fonts"));

    ["meiryo.ttc", "YuGothR.ttc", "YuGothM.ttc", "msgothic.ttc"]
        .into_iter()
        .map(|file_name| fonts_dir.join(file_name))
        .find_map(|path| fs::read(path).ok())
}
