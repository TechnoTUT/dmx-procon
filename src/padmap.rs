//! コントローラーの絵と、ボタンごとの割り当て(Switch の「ボタンの割り当てを変更」の画面のような形)。
//! 真ん中にコントローラーを描き、左右の枠から線でボタンにつなぐ。押している間は、枠・線・ボタンが光る。
//! 枠には、そのボタンでできることと、今の状態(今のゴボ・明るさ など)を出す。
//!
//! 絵は 900×430 の座標で決めておき、ウィンドウの大きさに合わせて縮める

use crate::control::Btn;
use crate::engine::Status;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

/// 絵の座標の大きさ
const W: f32 = 900.0;
const H: f32 = 430.0;
/// スティックが「倒れている」とみなす量
const STICK_ON: f32 = 0.2;

/// 押していることを表す色
const ON: Color32 = Color32::from_rgb(255, 196, 64);
const ON_FILL: Color32 = Color32::from_rgb(255, 238, 196);
/// ボタンの名前の札(Switch の画面の青)
const TAG: Color32 = Color32::from_rgb(44, 82, 230);

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
}

/// 枠1つ: どのボタンか、押しているか、何をするか
struct Item {
    side: Side,
    tag: &'static str,
    /// 線をつなぐ、絵の上の場所
    anchor: Pos2,
    active: bool,
    text: String,
    /// 枠に添える色(右スティックの、今の色)
    swatch: Option<Color32>,
}

fn items(s: &Status) -> Vec<Item> {
    let v = &s.view;
    let held = |b: Btn| s.held.contains(&b);
    let tilted = |(x, y): (f32, f32)| (x * x + y * y).sqrt() > STICK_ON;
    let on_off = |b: bool| if b { "ON" } else { "OFF" };
    let item = |side, tag, anchor: (f32, f32), active, text: String| Item { side, tag, anchor: pos2(anchor.0, anchor.1), active, text, swatch: None };
    use Side::*;
    vec![
        item(Left, "ZL", (366.0, 121.0), held(Btn::ZL), "今の色でフラッシュ(押す間)".into()),
        item(Left, "L", (378.0, 140.0), held(Btn::L), format!("ムービングを暗く(押す間){}", if v.movers_dark { "  暗くしている" } else { "" })),
        item(Left, "−", (421.0, 161.0), held(Btn::Minus), "パレットの色に戻す".into()),
        item(Left, "キャプチャ", (421.0, 178.0), held(Btn::Capture), format!("ジャイロで狙う: {}", on_off(v.gyro_aim))),
        item(Left, "Lスティック", (380.0, 186.0), tilted(s.left), if v.move_on { format!("向き  パン {:.0}%  チルト {:.0}%", v.pan * 100.0, v.tilt * 100.0) } else { "ムービングの向き".into() }),
        item(Left, "L押し込み", (380.0, 200.0), held(Btn::LStick), "ムービングを正面に".into()),
        item(Left, "十字 ↑↓", (422.0, 222.0), held(Btn::Up) || held(Btn::Down), format!("明るさ {:.0}%", v.master * 100.0)),
        item(Left, "十字 ←→", (410.0, 234.0), held(Btn::Left) || held(Btn::Right), "LED バーのエフェクト".into()),
        item(Right, "ZR", (534.0, 121.0), held(Btn::ZR), "白でフラッシュ(押す間)".into()),
        item(Right, "R", (522.0, 140.0), held(Btn::R), format!("ゴボを次へ  今: {}", v.gobo.as_deref().unwrap_or("-"))),
        item(Right, "+", (479.0, 161.0), held(Btn::Plus), if v.paused { "一時停止中(押すと再開)".into() } else { "一時停止".into() }),
        item(Right, "X", (524.0, 166.0), held(Btn::X), "ランダムな色".into()),
        item(Right, "A", (544.0, 186.0), held(Btn::A), "メイン色とサブ色を入れ替え".into()),
        item(Right, "B", (524.0, 206.0), held(Btn::B), format!("暗転{}", if v.faded { "中(押すと戻す)" } else { "" })),
        item(Right, "Y", (504.0, 186.0), held(Btn::Y), format!("色相を自動で回す: {}", on_off(v.auto_hue))),
        Item {
            swatch: v.color_on.then(|| Color32::from_rgb(v.rgb.0, v.rgb.1, v.rgb.2)),
            ..item(Right, "Rスティック", (486.0, 230.0), tilted(s.right), if v.color_on { format!("色  {:.0}° {:.0}%", v.hue * 360.0, v.sat * 100.0) } else { "色(倒すと選べる)".into() })
        },
        item(Right, "R押し込み", (486.0, 244.0), held(Btn::RStick), "白".into()),
    ]
}

/// 絵を描く。幅いっぱいに、縦横の比を保って広げる
pub fn show(ui: &mut egui::Ui, s: &Status) {
    let width = ui.available_width();
    let scale = (width / W).min(1.4);
    let (rect, _) = ui.allocate_exact_size(vec2(W * scale, H * scale), egui::Sense::hover());
    let at = |p: Pos2| rect.min + (p.to_vec2() * scale);
    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    let text = ui.visuals().text_color();
    let line = if dark { Color32::from_gray(120) } else { Color32::from_gray(150) };
    let box_fill = if dark { Color32::from_gray(38) } else { Color32::WHITE };
    let body = if dark { Color32::from_gray(70) } else { Color32::from_gray(245) };
    let outline = if dark { Color32::from_gray(170) } else { Color32::from_gray(60) };
    let items = items(s);

    // ---- 枠と線の位置(描くのは、線はコントローラーの本体のあと、枠は最後) ----
    let (box_w, box_h, gap) = (268.0, 36.0, 8.0);
    let column = |side: Side| items.iter().filter(move |i| i.side == side).enumerate();
    let mut boxes = Vec::new();
    let mut lines = Vec::new();
    for side in [Side::Left, Side::Right] {
        let n = items.iter().filter(|i| i.side == side).count() as f32;
        let top = (H - (n * box_h + (n - 1.0) * gap)) / 2.0;
        for (k, item) in column(side) {
            let y = top + k as f32 * (box_h + gap);
            let x = if side == Side::Left { 20.0 } else { W - 20.0 - box_w };
            let r = Rect::from_min_size(pos2(x, y), vec2(box_w, box_h));
            // 線: 枠の内側の辺から、少し横に出てから、ボタンへ
            let edge = if side == Side::Left { pos2(r.right(), r.center().y) } else { pos2(r.left(), r.center().y) };
            let elbow = pos2(if side == Side::Left { 304.0 } else { W - 304.0 }, r.center().y);
            let stroke = if item.active { Stroke::new(2.5 * scale, ON) } else { Stroke::new(1.0 * scale, line) };
            lines.push((vec![at(edge), at(elbow), at(item.anchor)], stroke));
            boxes.push((r, item));
        }
    }

    // ---- コントローラー ----
    let px = |v: f32| v * scale;
    let fill_stroke = Stroke::new(px(1.5), outline);
    let held = |b: Btn| s.held.contains(&b);
    // 肩のボタン(ZL・L・ZR・R)。本体の後ろから出ている形なので、先に描く
    for (r, b) in [
        (Rect::from_min_max(pos2(338.0, 112.0), pos2(395.0, 130.0)), Btn::ZL),
        (Rect::from_min_max(pos2(345.0, 128.0), pos2(412.0, 150.0)), Btn::L),
        (Rect::from_min_max(pos2(505.0, 112.0), pos2(562.0, 130.0)), Btn::ZR),
        (Rect::from_min_max(pos2(488.0, 128.0), pos2(555.0, 150.0)), Btn::R),
    ] {
        let r = Rect::from_min_max(at(r.min), at(r.max));
        painter.rect(r, CornerRadius::same(px(7.0) as u8), if held(b) { ON } else { body }, fill_stroke, StrokeKind::Inside);
    }
    // 本体と持ち手。外形を線の色で一回り大きく塗ってから、本体の色で塗る(つなぎ目に線が出ないように)
    // 持ち手は、下に向かって外へ開く(ハの字)
    let grips = [((372.0, 212.0), (336.0, 326.0)), ((528.0, 212.0), (564.0, 326.0))];
    for (grow, color) in [(px(1.5), outline), (0.0, body)] {
        let main = Rect::from_min_max(at(pos2(328.0, 145.0)), at(pos2(572.0, 260.0))).expand(grow);
        painter.rect_filled(main, CornerRadius::same((px(46.0) + grow) as u8), color);
        for (top, bottom) in grips {
            let (t, b) = (at(pos2(top.0, top.1)), at(pos2(bottom.0, bottom.1)));
            let (rt, rb) = (px(44.0) + grow, px(36.0) + grow);
            let n = (b - t).normalized().rot90();
            painter.add(Shape::convex_polygon(vec![t + n * rt, b + n * rb, b - n * rb, t - n * rt], color, Stroke::NONE));
            painter.circle_filled(t, rt, color);
            painter.circle_filled(b, rb, color);
        }
    }
    // 線は本体の上・ボタンの下に描き、ボタンまで届くようにする
    for (points, stroke) in lines {
        painter.add(Shape::line(points, stroke));
    }
    let button = |c: (f32, f32), r: f32, on: bool, label: &str| {
        let c = at(pos2(c.0, c.1));
        painter.circle(c, px(r), if on { ON } else { body }, fill_stroke);
        if !label.is_empty() {
            painter.text(c, egui::Align2::CENTER_CENTER, label, FontId::proportional(px(r * 1.15)), outline);
        }
    };
    // スティック: 外の輪と、倒した向きに動く頭
    for (c, stick, press) in [((380.0, 193.0), s.left, Btn::LStick), ((486.0, 237.0), s.right, Btn::RStick)] {
        let center = at(pos2(c.0, c.1));
        painter.circle(center, px(23.0), body, fill_stroke);
        let knob = center + vec2(stick.0, -stick.1) * px(9.0);
        let tilted = (stick.0 * stick.0 + stick.1 * stick.1).sqrt() > STICK_ON;
        painter.circle(knob, px(15.0), if held(press) || tilted { ON } else { body }, Stroke::new(px(2.0), outline));
    }
    // 十字キー
    let d = at(pos2(420.0, 236.0));
    for (b, off) in [(Btn::Up, vec2(0.0, -1.0)), (Btn::Down, vec2(0.0, 1.0)), (Btn::Left, vec2(-1.0, 0.0)), (Btn::Right, vec2(1.0, 0.0))] {
        let c = d + off * px(10.0);
        let r = Rect::from_center_size(c, Vec2::splat(px(11.0)));
        painter.rect(r, CornerRadius::same(px(2.0) as u8), if held(b) { ON } else { outline }, Stroke::NONE, StrokeKind::Inside);
    }
    painter.rect_filled(Rect::from_center_size(d, Vec2::splat(px(11.0))), 0.0, outline);
    // A・B・X・Y
    button((524.0, 166.0), 10.0, held(Btn::X), "X");
    button((544.0, 186.0), 10.0, held(Btn::A), "A");
    button((524.0, 206.0), 10.0, held(Btn::B), "B");
    button((504.0, 186.0), 10.0, held(Btn::Y), "Y");
    // −・+・キャプチャ・ホーム(ホームは使わない)
    button((421.0, 161.0), 6.0, held(Btn::Minus), "");
    button((479.0, 161.0), 6.0, held(Btn::Plus), "");
    let cap = Rect::from_center_size(at(pos2(421.0, 178.0)), Vec2::splat(px(11.0)));
    painter.rect(cap, CornerRadius::same(px(2.0) as u8), if held(Btn::Capture) { ON } else { body }, fill_stroke, StrokeKind::Inside);
    button((479.0, 178.0), 7.0, false, "");

    // ---- 枠の中身(線の上に描く) ----
    for (r, item) in boxes {
        let r = Rect::from_min_max(at(r.min), at(r.max));
        let (fill, stroke) = if item.active { (ON_FILL, Stroke::new(px(2.5), ON)) } else { (box_fill, Stroke::new(px(1.2), outline)) };
        painter.rect(r, CornerRadius::same(px(4.0) as u8), fill, stroke, StrokeKind::Inside);
        // ボタンの名前の札
        let font = FontId::proportional(px(13.0));
        let tag = painter.layout_no_wrap(item.tag.to_string(), font.clone(), Color32::WHITE);
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
