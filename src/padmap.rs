//! コントローラーの絵と、ボタンごとの割り当て。
//! 上にコントローラーを上から見た絵(ZL・L・ZR・R)、下に正面の絵を描き、まわりの枠から線でボタンにつなぐ。
//! A・B・X・Y の枠は、ボタンと同じひし形に並べる。押している間は、枠・線・ボタンが光る。
//! 枠には、そのボタンでできることと、今の状態(今のゴボ・明るさ など)を出す。
//!
//! 絵は 900×450 の座標で決めておき、ウィンドウの大きさに合わせて縮める

use crate::control::Btn;
use crate::engine::Status;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

/// 絵の座標の大きさ
const W: f32 = 900.0;
const H: f32 = 450.0;
/// スティックが「倒れている」とみなす量
const STICK_ON: f32 = 0.2;

/// 押していることを表す色
const ON: Color32 = Color32::from_rgb(255, 196, 64);
const ON_FILL: Color32 = Color32::from_rgb(255, 238, 196);
/// ボタンの名前の札
const TAG: Color32 = Color32::from_rgb(44, 82, 230);

/// 枠1つ: どこに置くか、どのボタンか、押しているか、何をするか
struct Item {
    rect: Rect,
    tag: &'static str,
    /// 線をつなぐ、絵の上の場所
    anchor: Pos2,
    active: bool,
    text: String,
    /// 枠に添える色(右スティックの、今の色)
    swatch: Option<Color32>,
}

/// 枠の場所(x, y, 幅)。高さはどれも 36
fn r(x: f32, y: f32, w: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, 36.0))
}

// 正面の絵の、ボタンの場所
const LSTICK: (f32, f32) = (380.0, 253.0);
const RSTICK: (f32, f32) = (486.0, 297.0);
const DPAD: (f32, f32) = (420.0, 296.0);
const X_BTN: (f32, f32) = (524.0, 226.0);
const A_BTN: (f32, f32) = (544.0, 246.0);
const B_BTN: (f32, f32) = (524.0, 266.0);
const Y_BTN: (f32, f32) = (504.0, 246.0);
const MINUS: (f32, f32) = (421.0, 221.0);
const PLUS: (f32, f32) = (479.0, 221.0);
const CAPTURE: (f32, f32) = (421.0, 238.0);
const HOME: (f32, f32) = (479.0, 238.0);
// 上から見た絵の、ZL・L・ZR・R
const ZL_RECT: [f32; 4] = [352.0, 34.0, 424.0, 74.0];
const L_RECT: [f32; 4] = [352.0, 78.0, 424.0, 98.0];
const ZR_RECT: [f32; 4] = [476.0, 34.0, 548.0, 74.0];
const R_RECT: [f32; 4] = [476.0, 78.0, 548.0, 98.0];

fn rect4(a: [f32; 4]) -> Rect {
    Rect::from_min_max(pos2(a[0], a[1]), pos2(a[2], a[3]))
}

fn p(a: (f32, f32)) -> Pos2 {
    pos2(a.0, a.1)
}

fn items(s: &Status) -> Vec<Item> {
    let v = &s.view;
    let held = |b: Btn| s.held.contains(&b);
    let tilted = |(x, y): (f32, f32)| (x * x + y * y).sqrt() > STICK_ON;
    let on_off = |b: bool| if b { "ON" } else { "OFF" };
    let item = |rect, tag, anchor, active, text: String| Item { rect, tag, anchor, active, text, swatch: None };
    vec![
        // 左上・右上: 上から見た絵の ZL・L・ZR・R
        item(r(30.0, 32.0, 270.0), "ZL", rect4(ZL_RECT).left_center(), held(Btn::ZL), "今の色でフラッシュ(押す間)".into()),
        item(r(30.0, 76.0, 270.0), "L", rect4(L_RECT).left_center(), held(Btn::L), format!("ムービングを暗く(押す間){}", if v.movers_dark { "  暗い" } else { "" })),
        item(r(600.0, 32.0, 270.0), "ZR", rect4(ZR_RECT).right_center(), held(Btn::ZR), "白でフラッシュ(押す間)".into()),
        item(r(600.0, 76.0, 270.0), "R", rect4(R_RECT).right_center(), held(Btn::R), format!("ゴボを次へ  今: {}", v.gobo.as_deref().unwrap_or("-"))),
        // 左: 正面の −・キャプチャ・L スティック・十字キー
        item(r(30.0, 150.0, 270.0), "−", p(MINUS), held(Btn::Minus), "パレットの色に戻す".into()),
        item(r(30.0, 194.0, 270.0), "キャプチャ", p(CAPTURE), held(Btn::Capture), format!("ジャイロで狙う: {}", on_off(v.gyro_aim))),
        item(
            r(30.0, 238.0, 270.0),
            "Lスティック",
            p(LSTICK),
            tilted(s.left),
            if v.move_on { format!("パン {:.0}%  チルト {:.0}%", v.pan * 100.0, v.tilt * 100.0) } else { "ムービングの向き".into() },
        ),
        item(r(30.0, 282.0, 270.0), "L押し込み", p(LSTICK), held(Btn::LStick), "ムービングを正面に".into()),
        item(r(30.0, 340.0, 270.0), "十字 ↑↓", p(DPAD) + vec2(0.0, -8.0), held(Btn::Up) || held(Btn::Down), format!("明るさ {:.0}%", v.master * 100.0)),
        item(r(30.0, 384.0, 270.0), "十字 ←→", p(DPAD) + vec2(-8.0, 0.0), held(Btn::Left) || held(Btn::Right), "LED バーのエフェクト".into()),
        // 右: A・B・X・Y はボタンと同じひし形に並べる
        item(r(670.0, 140.0, 150.0), "X", p(X_BTN), held(Btn::X), "ランダムな色".into()),
        item(r(600.0, 184.0, 140.0), "Y", p(Y_BTN), held(Btn::Y), format!("色相回転 {}", on_off(v.auto_hue))),
        item(r(746.0, 184.0, 140.0), "A", p(A_BTN), held(Btn::A), "メイン⇄サブ".into()),
        item(r(670.0, 228.0, 150.0), "B", p(B_BTN), held(Btn::B), if v.faded { "暗転中".into() } else { "暗転".into() }),
        // 右下: +・R スティック
        item(r(600.0, 296.0, 270.0), "+", p(PLUS), held(Btn::Plus), if v.paused { "一時停止中(押すと再開)".into() } else { "一時停止".into() }),
        Item {
            swatch: v.color_on.then(|| Color32::from_rgb(v.rgb.0, v.rgb.1, v.rgb.2)),
            ..item(r(600.0, 340.0, 270.0), "Rスティック", p(RSTICK), tilted(s.right), if v.color_on { format!("色  {}°  {:.0}%", (v.hue * 360.0).round() as i32 % 360, v.sat * 100.0) } else { "色(倒すと選べる)".into() })
        },
        item(r(600.0, 384.0, 270.0), "R押し込み", p(RSTICK), held(Btn::RStick), "白".into()),
    ]
}

/// 絵を描く。幅いっぱいに、縦横の比を保って広げる
pub fn show(ui: &mut egui::Ui, s: &Status) {
    let scale = (ui.available_width() / W).min(ui.available_height().max(200.0) / H).min(1.5);
    let (rect, _) = ui.allocate_exact_size(vec2(W * scale, H * scale), egui::Sense::hover());
    let at = |q: Pos2| rect.min + (q.to_vec2() * scale);
    let px = |v: f32| v * scale;
    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    let text = ui.visuals().text_color();
    let line = if dark { Color32::from_gray(120) } else { Color32::from_gray(150) };
    let box_fill = if dark { Color32::from_gray(38) } else { Color32::WHITE };
    let body = if dark { Color32::from_gray(70) } else { Color32::from_gray(245) };
    let outline = if dark { Color32::from_gray(170) } else { Color32::from_gray(60) };
    let fill_stroke = Stroke::new(px(1.5), outline);
    let held = |b: Btn| s.held.contains(&b);
    let items = items(s);

    // ---- 上から見た絵: 本体の上の辺と、ZL・L・ZR・R ----
    for (grow, color) in [(px(1.5), outline), (0.0, body)] {
        let top = Rect::from_min_max(at(pos2(330.0, 46.0)), at(pos2(570.0, 112.0))).expand(grow);
        painter.rect_filled(top, CornerRadius::same((px(26.0) + grow) as u8), color);
        for c in [(350.0, 82.0), (550.0, 82.0)] {
            painter.circle_filled(at(p(c)), px(34.0) + grow, color);
        }
    }
    for (a, b, label) in [(ZL_RECT, Btn::ZL, "ZL"), (L_RECT, Btn::L, "L"), (ZR_RECT, Btn::ZR, "ZR"), (R_RECT, Btn::R, "R")] {
        let r = Rect::from_min_max(at(rect4(a).min), at(rect4(a).max));
        painter.rect(r, CornerRadius::same(px(8.0) as u8), if held(b) { ON } else { outline }, Stroke::NONE, StrokeKind::Inside);
        painter.text(r.center(), egui::Align2::CENTER_CENTER, label, FontId::proportional(px(if r.height() > px(30.0) { 16.0 } else { 11.0 })), if held(b) { Color32::BLACK } else { body });
    }
    painter.text(at(pos2(450.0, 130.0)), egui::Align2::CENTER_CENTER, "上から見たところ", FontId::proportional(px(11.0)), line);

    // ---- 正面の絵 ----
    // 肩のボタン(正面からは、本体の後ろに少し見える)
    for (a, b) in [([345.0, 186.0, 412.0, 210.0], Btn::L), ([488.0, 186.0, 555.0, 210.0], Btn::R)] {
        let r = Rect::from_min_max(at(rect4(a).min), at(rect4(a).max));
        painter.rect(r, CornerRadius::same(px(7.0) as u8), if held(b) { ON } else { body }, fill_stroke, StrokeKind::Inside);
    }
    // 本体と持ち手(下に向かって外へ開く)。外形を線の色で一回り大きく塗ってから、本体の色で塗る
    let grips = [((372.0, 272.0), (336.0, 386.0)), ((528.0, 272.0), (564.0, 386.0))];
    for (grow, color) in [(px(1.5), outline), (0.0, body)] {
        let main = Rect::from_min_max(at(pos2(328.0, 205.0)), at(pos2(572.0, 320.0))).expand(grow);
        painter.rect_filled(main, CornerRadius::same((px(46.0) + grow) as u8), color);
        for (top, bottom) in grips {
            let (t, b) = (at(p(top)), at(p(bottom)));
            let (rt, rb) = (px(44.0) + grow, px(36.0) + grow);
            let n = (b - t).normalized().rot90();
            painter.add(Shape::convex_polygon(vec![t + n * rt, b + n * rb, b - n * rb, t - n * rt], color, Stroke::NONE));
            painter.circle_filled(t, rt, color);
            painter.circle_filled(b, rb, color);
        }
    }

    // ---- 線(本体の上・ボタンの下に描き、ボタンまで届くようにする) ----
    for item in &items {
        // 枠の、絵に近い側の辺から、少し横に出てから、ボタンへ
        let left = item.rect.center().x < W / 2.0;
        let edge = pos2(if left { item.rect.right() } else { item.rect.left() }, item.rect.center().y);
        let elbow = edge + vec2(if left { 14.0 } else { -14.0 }, 0.0);
        let stroke = if item.active { Stroke::new(px(2.5), ON) } else { Stroke::new(px(1.0), line) };
        painter.add(Shape::line(vec![at(edge), at(elbow), at(item.anchor)], stroke));
    }

    // ---- 正面のボタン ----
    let button = |c: (f32, f32), r: f32, on: bool, label: &str| {
        let c = at(p(c));
        painter.circle(c, px(r), if on { ON } else { body }, fill_stroke);
        if !label.is_empty() {
            painter.text(c, egui::Align2::CENTER_CENTER, label, FontId::proportional(px(r * 1.15)), outline);
        }
    };
    // スティック: 外の輪と、倒した向きに動く頭
    for (c, stick, press) in [(LSTICK, s.left, Btn::LStick), (RSTICK, s.right, Btn::RStick)] {
        let center = at(p(c));
        painter.circle(center, px(23.0), body, fill_stroke);
        let knob = center + vec2(stick.0, -stick.1) * px(9.0);
        let tilted = (stick.0 * stick.0 + stick.1 * stick.1).sqrt() > STICK_ON;
        painter.circle(knob, px(15.0), if held(press) || tilted { ON } else { body }, Stroke::new(px(2.0), outline));
    }
    // 十字キー
    let d = at(p(DPAD));
    for (b, off) in [(Btn::Up, vec2(0.0, -1.0)), (Btn::Down, vec2(0.0, 1.0)), (Btn::Left, vec2(-1.0, 0.0)), (Btn::Right, vec2(1.0, 0.0))] {
        let r = Rect::from_center_size(d + off * px(10.0), Vec2::splat(px(11.0)));
        painter.rect(r, CornerRadius::same(px(2.0) as u8), if held(b) { ON } else { outline }, Stroke::NONE, StrokeKind::Inside);
    }
    painter.rect_filled(Rect::from_center_size(d, Vec2::splat(px(11.0))), 0.0, outline);
    button(X_BTN, 10.0, held(Btn::X), "X");
    button(A_BTN, 10.0, held(Btn::A), "A");
    button(B_BTN, 10.0, held(Btn::B), "B");
    button(Y_BTN, 10.0, held(Btn::Y), "Y");
    button(MINUS, 6.0, held(Btn::Minus), "");
    button(PLUS, 6.0, held(Btn::Plus), "");
    let cap = Rect::from_center_size(at(p(CAPTURE)), Vec2::splat(px(11.0)));
    painter.rect(cap, CornerRadius::same(px(2.0) as u8), if held(Btn::Capture) { ON } else { body }, fill_stroke, StrokeKind::Inside);
    // ホームは使わない
    button(HOME, 7.0, false, "");

    // ---- 枠(いちばん上に描く) ----
    for item in &items {
        let r = Rect::from_min_max(at(item.rect.min), at(item.rect.max));
        let (fill, stroke) = if item.active { (ON_FILL, Stroke::new(px(2.5), ON)) } else { (box_fill, Stroke::new(px(1.2), outline)) };
        painter.rect(r, CornerRadius::same(px(4.0) as u8), fill, stroke, StrokeKind::Inside);
        // ボタンの名前の札
        let tag = painter.layout_no_wrap(item.tag.to_string(), FontId::proportional(px(13.0)), Color32::WHITE);
        let tag_rect = Rect::from_min_size(r.min + vec2(px(6.0), (r.height() - tag.size().y - px(6.0)) / 2.0), tag.size() + vec2(px(10.0), px(6.0)));
        painter.rect_filled(tag_rect, CornerRadius::same(px(4.0) as u8), TAG);
        painter.galley(tag_rect.min + vec2(px(5.0), px(3.0)), tag, Color32::WHITE);
        // 色の見本
        let mut x = tag_rect.right() + px(8.0);
        if let Some(c) = item.swatch {
            let sw = Rect::from_min_size(pos2(x, r.center().y - px(8.0)), vec2(px(16.0), px(16.0)));
            painter.rect(sw, CornerRadius::same(px(3.0) as u8), c, Stroke::new(px(1.0), outline), StrokeKind::Inside);
            x = sw.right() + px(6.0);
        }
        let text_color = if item.active { Color32::from_gray(20) } else { text };
        let galley = painter.layout(item.text.clone(), FontId::proportional(px(13.0)), text_color, r.right() - x - px(6.0));
        painter.galley(pos2(x, r.center().y - galley.size().y / 2.0), galley, text_color);
    }
}
