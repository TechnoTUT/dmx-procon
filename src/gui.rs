//! ウィンドウ。今の色・明るさ・ムービングの向き・最後にした操作と、送り先などの設定を出す。
//! コントローラーを読んで送るのは engine.rs で、別のスレッドで動かす

use crate::engine::{self, Options, Shared, lock};
use crate::fonts;
use eframe::egui::{self, Color32, RichText};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;

/// このアプリの設定ファイル(送り先を覚えておく)。Mac: ~/Library/Application Support/procon-light/settings.json など
fn settings_path() -> std::path::PathBuf {
    let base = crate::rig::config_dir();
    base.parent().map(|p| p.join("procon-light")).unwrap_or_else(|| base.join("procon-light")).join("settings.json")
}

pub fn saved_to() -> Option<String> {
    let text = std::fs::read_to_string(settings_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["to"].as_str().filter(|s| !s.is_empty()).map(String::from)
}

fn save_to(to: &str) {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::json!({ "to": to }).to_string());
}

struct App {
    shared: Arc<Shared>,
    engine: Option<JoinHandle<Result<(), String>>>,
    /// 送り先の入力欄
    to_edit: String,
    /// engine が止まったときの理由
    fatal: Option<String>,
    /// 開発・確認用の、画面の保存(PROCON_LIGHT_SHOT)
    shot: Option<crate::shot::Shot>,
}

pub fn run(opts: Options) -> Result<(), String> {
    let (ctl, shared) = engine::setup(&opts, saved_to());
    let shared = Arc::new(shared);
    let to_edit = lock(&shared.settings).to.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 640.0]).with_min_inner_size([640.0, 420.0]).with_title("procon-light").with_app_id("procon-light"),
        ..Default::default()
    };
    eframe::run_native(
        "procon-light",
        options,
        Box::new(move |cc| {
            fonts::setup(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let shared2 = shared.clone();
            let engine = std::thread::Builder::new()
                .name("engine".into())
                .spawn(move || engine::run(&shared2, ctl, &|| ctx.request_repaint()))
                .map_err(|e| e.to_string())?;
            Ok(Box::new(App { shared, engine: Some(engine), to_edit, fatal: None, shot: crate::shot::Shot::from_env() }))
        }),
    )
    .map_err(|e| e.to_string())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(s) = &mut self.shot {
            s.tick(ui.ctx());
        }
        // engine が終わっていたら(SDL を使えないなど)、理由を出す
        if self.engine.as_ref().is_some_and(|h| h.is_finished())
            && let Some(h) = self.engine.take()
        {
            self.fatal = Some(match h.join() {
                Ok(Ok(())) => "止まりました".into(),
                Ok(Err(e)) => e,
                Err(_) => "内部でエラーが起きて止まりました".into(),
            });
        }
        let status = lock(&self.shared.status).clone();
        let settings = lock(&self.shared.settings).clone();

        // 下: 設定
        egui::Panel::bottom("settings").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("送り先(dmxctl の OSC)");
                let edit = ui.add(egui::TextEdit::singleline(&mut self.to_edit).desired_width(170.0));
                let apply = ui.add_enabled(self.to_edit.trim() != settings.to, egui::Button::new("変える"));
                if apply.clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    let to = self.to_edit.trim().to_string();
                    lock(&self.shared.settings).to = to.clone();
                    save_to(&to);
                }
                ui.add_space(12.0);
                let mut test = settings.test;
                if ui.checkbox(&mut test, "テスト(送らずに、入力だけを見る)").changed() {
                    lock(&self.shared.settings).test = test;
                }
            });
            if let Some(e) = &status.error {
                ui.colored_label(Color32::from_rgb(220, 70, 70), e);
            }
            if status.osc_off {
                ui.colored_label(Color32::from_rgb(210, 130, 30), "dmxctl の OSC 受信が OFF です。dmxctl の設定で ON にしてください");
            }
            ui.label(RichText::new(format!("ムービング {}台 / LED バーのエフェクト {}個", status.movers.len(), status.pixel_fx)).weak());
            ui.add_space(4.0);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // 上: コントローラーと、最後にした操作
            ui.horizontal(|ui| {
                let (dot, text) = match (&status.pad, status.demo) {
                    (_, true) => (Color32::from_rgb(90, 170, 255), "デモ(コントローラーを使わない)".to_string()),
                    (Some(name), _) => (Color32::from_rgb(60, 180, 100), name.clone()),
                    (None, _) => (Color32::from_rgb(230, 150, 40), "コントローラーを待っています".to_string()),
                };
                ui.label(RichText::new("●").color(dot).size(18.0));
                ui.label(RichText::new(text).strong().size(16.0));
                if status.view.paused {
                    ui.label(RichText::new("一時停止中").color(Color32::from_rgb(220, 70, 70)).strong());
                }
                if settings.test {
                    ui.label(RichText::new("テスト中(送っていない)").color(Color32::from_rgb(210, 130, 30)).strong());
                }
            });
            if let Some(e) = &self.fatal {
                ui.colored_label(Color32::from_rgb(220, 70, 70), e);
            }
            ui.label(RichText::new(&status.notice).size(15.0));
            ui.add_space(4.0);
            // 真ん中: コントローラーの絵と、ボタンの割り当て
            crate::padmap::show(ui, &status);
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // 押していたフラッシュなどを戻してから終わる(engine が終わるときに送る)
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.engine.take() {
            let _ = h.join();
        }
    }
}
