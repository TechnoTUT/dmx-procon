//! 日本語を出すため、OS に入っている日本語フォントを読み込む(dmxctl の src/theme.rs と同じ候補)

use eframe::egui::{self, FontFamily};
use std::sync::Arc;

/// 候補。Windows: メイリオ・游ゴシック / Mac: ヒラギノ / Linux: Noto CJK など
const JP_FONTS: &[&str] = &[
    "C:/Windows/Fonts/meiryo.ttc",
    "C:/Windows/Fonts/YuGothM.ttc",
    "C:/Windows/Fonts/msgothic.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/ipafont-gothic/ipagp.ttf",
    "/usr/share/fonts/truetype/takao-gothic/TakaoPGothic.ttf",
    "/usr/share/fonts/truetype/vlgothic/VL-PGothic-Regular.ttf",
];

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Linux で上のどれも無いときは、fontconfig に日本語フォントを聞く
    let candidates = JP_FONTS.iter().map(|s| s.to_string()).chain(std::iter::once_with(fontconfig_jp).flatten());
    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        fonts.font_data.insert("jp".into(), Arc::new(egui::FontData::from_owned(bytes)));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "jp".into());
        }
        if let Some(list) = fonts.families.get_mut(&FontFamily::Monospace) {
            list.push("jp".into());
        }
        break;
    }
    ctx.set_fonts(fonts);
}

/// fontconfig(fc-match)が選ぶ日本語フォントのファイル。Linux 以外・見つからないときは None
fn fontconfig_jp() -> Option<String> {
    if cfg!(any(windows, target_os = "macos")) {
        return None;
    }
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}", "sans-serif:lang=ja"]).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}
