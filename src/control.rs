//! コントローラーの入力から、dmxctl へ送る OSC を決める(SDL から切り離してあるので、テストできる)。
//!
//! | 操作 | 照明 |
//! |---|---|
//! | 右スティック | 色。倒す向きが色相(上=赤から時計まわり)、倒し具合が鮮やかさ。離してもその色のまま |
//! | 右スティック押し込み | 白 |
//! | 左スティック | ムービングの向き。倒した方へ動き、離すと止まる |
//! | 左スティック押し込み | ムービングを正面に戻す |
//! | ZR / ZL(押している間) | 白でフラッシュ / 今の色でフラッシュ |
//! | R | ムービングのゴボを次へ(オープン → ゴボ1 → … → オープン) |
//! | L(押している間) | ムービングを暗くする。離すと元の明るさに戻す |
//! | A | メイン色とサブ色を入れ替え |
//! | B | 暗転フェード(押すたびに暗くする・戻す) |
//! | X | ランダムな色 |
//! | Y | 色相を自動で回す(ON/OFF) |
//! | 十字 上・下 | 全体の明るさ |
//! | 十字 左・右 | LED バーのエフェクトを前・次へ |
//! | − | スティックの色をやめて、dmxctl のパレットの色に戻す |
//! | + | 一時停止(送るのを止める・再開する) |
//! | キャプチャ | ジャイロでムービングを狙う(ON/OFF) |
//! | HOME(押している間) | ARM済みのレーザーを出す。離すと止まる(dmxctl で ARM していなければ出ない) |

use crate::osc::{Arg, Msg};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Btn {
    A,
    B,
    X,
    Y,
    L,
    R,
    ZL,
    ZR,
    LStick,
    RStick,
    Up,
    Down,
    Left,
    Right,
    Minus,
    Plus,
    Capture,
    Home,
}

/// 1回分の入力。スティックは -1〜1 で、上と右がプラス
#[derive(Debug, Clone, Default)]
pub struct Input {
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub held: Vec<Btn>,
    /// ジャイロ(ラジアン/秒)。ピッチ(手前・奥に傾ける)とヨー(左右にひねる)
    pub gyro: Option<(f32, f32)>,
}

impl Input {
    fn has(&self, b: Btn) -> bool {
        self.held.contains(&b)
    }
}

/// コントローラーへの手ごたえ(振動)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rumble {
    pub strength: f32,
    pub ms: u32,
}

/// スティックの、効かない真ん中の範囲
const DEADZONE: f32 = 0.2;
/// 色相を自動で回すとき、1周にかける秒数
const HUE_CYCLE_SECS: f32 = 12.0;
/// 左スティックをいっぱいに倒したとき、1秒で動く量(パン・チルトの全体を 1 として)
const MOVE_SPEED: f32 = 0.5;
/// ジャイロで狙うとき、コントローラーを1ラジアン回したときに動く量
const GYRO_GAIN: f32 = 1.0 / PI;
/// 色・向きを送る間隔の下限(秒)。dmxctl の出力は 40Hz なので、それより細かく送っても変わらない
const SEND_EVERY: f32 = 1.0 / 30.0;
/// dmxctl 側で、受けた色へ近づける秒数(少しだけなめらかにする)
const COLOR_SMOOTH: f32 = 0.08;
/// HOME を押している間、レーザーの合図を送り直す間隔(秒)。dmxctl は1秒届かないと止めるので、それより短く
const LASER_EVERY: f32 = 0.3;

#[derive(Debug, Clone)]
pub struct Controller {
    pub hue: f32,
    pub sat: f32,
    /// スティックの色を dmxctl に送っているか(− で dmxctl のパレットに戻すと false)
    pub color_on: bool,
    pub auto_hue: bool,
    pub master: f32,
    pub faded: bool,
    pub paused: bool,
    pub gyro_aim: bool,
    pub pan: f32,
    pub tilt: f32,
    /// 向きを dmxctl に送っているか(左スティック・ジャイロで動かすまでは送らない。起動しただけで向きを変えないため)
    pub move_on: bool,
    pub movers: Vec<u32>,
    /// ムービングの元の明るさ(L を離したときに戻す値)。無いムービングは 1.0 に戻す
    pub dimmers: Vec<(u32, f32)>,
    /// R で切り替えるゴボ(名前と値)と、今のゴボ
    pub gobos: Vec<(String, f32)>,
    pub gobo_index: Option<usize>,
    /// L でムービングを暗くしているか
    pub movers_dark: bool,
    /// HOME でレーザーを出しているか
    pub laser: bool,
    since_laser: f32,
    pub pixel_fx: Vec<String>,
    /// 今 ON にしている LED バーのエフェクト(pixel_fx の番号)
    pub fx_index: Option<usize>,
    /// 画面に出す、最後にした操作
    pub last: String,
    prev: Vec<Btn>,
    since_color: f32,
    since_move: f32,
    sent_color: Option<(u8, u8, u8)>,
    sent_move: Option<(u8, u8)>,
    seed: u64,
}

impl Controller {
    pub fn new(movers: Vec<u32>, pixel_fx: Vec<String>, seed: u64) -> Self {
        Controller {
            hue: 0.0,
            sat: 0.0,
            color_on: false,
            auto_hue: false,
            master: 1.0,
            faded: false,
            paused: false,
            gyro_aim: false,
            pan: 0.5,
            tilt: 0.5,
            move_on: false,
            movers,
            dimmers: Vec::new(),
            gobos: Vec::new(),
            gobo_index: None,
            movers_dark: false,
            laser: false,
            since_laser: 0.0,
            pixel_fx,
            fx_index: None,
            last: String::new(),
            prev: Vec::new(),
            since_color: f32::INFINITY,
            since_move: f32::INFINITY,
            sent_color: None,
            sent_move: None,
            seed: seed | 1,
        }
    }

    /// ムービングの元の明るさと、切り替えるゴボを渡す(dmxctl の設定から読んだもの)
    pub fn with_mover_info(mut self, dimmers: Vec<(u32, f32)>, gobos: Vec<(String, f32)>) -> Self {
        self.dimmers = dimmers;
        self.gobos = gobos;
        self
    }

    /// ムービングを暗くする・元の明るさに戻す
    fn movers_dimmer(&self, dark: bool) -> Vec<Msg> {
        self.movers
            .iter()
            .map(|id| {
                let restore = self.dimmers.iter().find(|d| d.0 == *id).map_or(1.0, |d| d.1);
                Msg::f(&format!("/dmx/fixture/{id}/dimmer"), if dark { 0.0 } else { restore })
            })
            .collect()
    }

    /// 今の色(0〜255)
    pub fn rgb(&self) -> (u8, u8, u8) {
        let (r, g, b) = hsv(self.hue, self.sat);
        ((r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
    }

    /// 入力を1回分進める。`dt` は前の呼び出しからの秒数。送るものと、振動を返す
    pub fn step(&mut self, input: &Input, dt: f32) -> (Vec<Msg>, Option<Rumble>) {
        let mut out = Vec::new();
        let mut rumble = None;
        let pressed = |b: Btn| input.has(b) && !self.prev.contains(&b);
        let released = |b: Btn| !input.has(b) && self.prev.contains(&b);
        let (p, r): (Vec<Btn>, Vec<Btn>) = (ALL.iter().copied().filter(|&b| pressed(b)).collect(), ALL.iter().copied().filter(|&b| released(b)).collect());
        self.prev = input.held.clone();

        // 一時停止は、止めている間も切り替えられるように先に見る
        if p.contains(&Btn::Plus) {
            self.paused = !self.paused;
            self.last = if self.paused { "一時停止".into() } else { "再開".into() };
            rumble = Some(Rumble { strength: 0.3, ms: 80 });
            // 止める前に押していたフラッシュ・暗くしていたムービングは、離したことにしておく
            if self.paused {
                out.push(Msg::f("/dmx/flash", 0.0));
                out.push(Msg::f("/dmx/flash/color", 0.0));
                if self.laser {
                    self.laser = false;
                    out.push(Msg::f("/dmx/laser", 0.0));
                }
                if self.movers_dark {
                    self.movers_dark = false;
                    out.extend(self.movers_dimmer(false));
                }
            }
            return (out, rumble);
        }
        if self.paused {
            return (out, rumble);
        }

        // ---- 色 ----
        let (rx, ry) = input.right;
        let m = (rx * rx + ry * ry).sqrt();
        if m > DEADZONE {
            // 上を 0 として時計まわりに一周
            self.hue = (rx.atan2(ry) / (2.0 * PI)).rem_euclid(1.0);
            self.sat = ((m - DEADZONE) / (0.95 - DEADZONE)).clamp(0.0, 1.0);
            self.color_on = true;
        } else if self.auto_hue && self.color_on {
            self.hue = (self.hue + dt / HUE_CYCLE_SECS).rem_euclid(1.0);
        }
        if p.contains(&Btn::RStick) {
            self.sat = 0.0;
            self.color_on = true;
            self.last = "白".into();
        }
        if p.contains(&Btn::X) {
            self.hue = self.random();
            self.sat = 1.0;
            self.color_on = true;
            self.last = "ランダムな色".into();
            rumble = Some(Rumble { strength: 0.25, ms: 60 });
        }
        if p.contains(&Btn::Y) {
            self.auto_hue = !self.auto_hue;
            if self.auto_hue && !self.color_on {
                self.color_on = true;
                self.sat = 1.0;
            }
            self.last = if self.auto_hue { "色相を自動で回す: ON".into() } else { "色相を自動で回す: OFF".into() };
        }
        if p.contains(&Btn::Minus) {
            self.color_on = false;
            self.auto_hue = false;
            self.sent_color = None;
            out.push(Msg::new("/dmx/rgb/off", vec![]));
            self.last = "パレットの色に戻す".into();
        }
        self.since_color += dt;
        if self.color_on && self.since_color >= SEND_EVERY {
            let rgb = self.rgb();
            if self.sent_color != Some(rgb) {
                let f = |v: u8| Arg::F(v as f32 / 255.0);
                out.push(Msg::new("/dmx/main/rgb", vec![f(rgb.0), f(rgb.1), f(rgb.2), Arg::F(COLOR_SMOOTH)]));
                self.sent_color = Some(rgb);
                self.since_color = 0.0;
            }
        }

        // ---- 向き ----
        if !self.movers.is_empty() {
            let (lx, ly) = input.left;
            let dz = |v: f32| if v.abs() > DEADZONE { (v.abs() - DEADZONE) / (1.0 - DEADZONE) * v.signum() } else { 0.0 };
            if dz(lx) != 0.0 || dz(ly) != 0.0 {
                self.move_on = true;
            }
            self.pan += dz(lx) * MOVE_SPEED * dt;
            self.tilt += dz(ly) * MOVE_SPEED * dt;
            if p.contains(&Btn::Capture) {
                self.gyro_aim = !self.gyro_aim;
                self.last = if self.gyro_aim { "ジャイロで狙う: ON".into() } else { "ジャイロで狙う: OFF".into() };
                rumble = Some(Rumble { strength: 0.4, ms: 120 });
            }
            if self.gyro_aim
                && let Some((pitch, yaw)) = input.gyro
            {
                // 右にひねると右へ、奥に傾けると上へ
                self.move_on = true;
                self.pan -= yaw * GYRO_GAIN * dt;
                self.tilt += pitch * GYRO_GAIN * dt;
            }
            if p.contains(&Btn::LStick) {
                self.pan = 0.5;
                self.tilt = 0.5;
                self.move_on = true;
                self.last = "ムービングを正面に".into();
            }
            self.pan = self.pan.clamp(0.0, 1.0);
            self.tilt = self.tilt.clamp(0.0, 1.0);
            self.since_move += dt;
            let now = ((self.pan * 255.0).round() as u8, (self.tilt * 255.0).round() as u8);
            if self.move_on && self.since_move >= SEND_EVERY && self.sent_move != Some(now) {
                for id in &self.movers {
                    out.push(Msg::f(&format!("/dmx/fixture/{id}/pan"), self.pan));
                    out.push(Msg::f(&format!("/dmx/fixture/{id}/tilt"), self.tilt));
                }
                self.sent_move = Some(now);
                self.since_move = 0.0;
            }
        }

        // ---- 押している間だけ ----
        for (btn, addr, name) in [(Btn::ZR, "/dmx/flash", "白フラッシュ"), (Btn::ZL, "/dmx/flash/color", "色フラッシュ")] {
            if p.contains(&btn) {
                out.push(Msg::f(addr, 1.0));
                self.last = name.into();
                rumble = Some(Rumble { strength: 0.6, ms: 90 });
            }
            if r.contains(&btn) {
                out.push(Msg::f(addr, 0.0));
            }
        }

        // HOME: 押している間だけレーザー。送り続けないと dmxctl が止めるので、押している間は送り直す
        if p.contains(&Btn::Home) {
            self.laser = true;
            self.since_laser = 0.0;
            out.push(Msg::f("/dmx/laser", 1.0));
            self.last = "レーザー".into();
            rumble = Some(Rumble { strength: 0.5, ms: 60 });
        } else if self.laser && input.has(Btn::Home) {
            self.since_laser += dt;
            if self.since_laser >= LASER_EVERY {
                self.since_laser = 0.0;
                out.push(Msg::f("/dmx/laser", 1.0));
            }
        }
        if r.contains(&Btn::Home) && self.laser {
            self.laser = false;
            out.push(Msg::f("/dmx/laser", 0.0));
        }

        // ---- 1回押す ----
        if p.contains(&Btn::R) && !self.gobos.is_empty() && !self.movers.is_empty() {
            let next = self.gobo_index.map_or(1.min(self.gobos.len() - 1), |i| (i + 1) % self.gobos.len());
            let (name, v) = self.gobos[next].clone();
            for id in &self.movers {
                out.push(Msg::f(&format!("/dmx/fixture/{id}/gobo"), v));
            }
            self.gobo_index = Some(next);
            self.last = format!("ゴボ: {name}");
            rumble = Some(Rumble { strength: 0.35, ms: 40 });
        }
        if !self.movers.is_empty() {
            if p.contains(&Btn::L) {
                self.movers_dark = true;
                out.extend(self.movers_dimmer(true));
                self.last = "ムービングを暗く".into();
            }
            if r.contains(&Btn::L) && self.movers_dark {
                self.movers_dark = false;
                out.extend(self.movers_dimmer(false));
            }
        }
        if p.contains(&Btn::A) {
            out.push(Msg::new("/dmx/swap", vec![]));
            self.last = "メイン⇄サブ".into();
        }
        if p.contains(&Btn::B) {
            self.faded = !self.faded;
            out.push(Msg::f("/dmx/fade", if self.faded { 1.0 } else { 0.0 }));
            self.last = if self.faded { "暗くする".into() } else { "明るく戻す".into() };
        }
        for (btn, step) in [(Btn::Up, 0.1f32), (Btn::Down, -0.1)] {
            if p.contains(&btn) {
                self.master = ((self.master + step) * 10.0).round().clamp(0.0, 10.0) / 10.0;
                out.push(Msg::f("/dmx/master", self.master));
                self.last = format!("明るさ {:.0}%", self.master * 100.0);
            }
        }
        for (btn, dir) in [(Btn::Right, 1isize), (Btn::Left, -1)] {
            if p.contains(&btn) && !self.pixel_fx.is_empty() {
                let n = self.pixel_fx.len() as isize;
                let next = match self.fx_index {
                    Some(i) => (i as isize + dir).rem_euclid(n) as usize,
                    None => if dir > 0 { 0 } else { (n - 1) as usize },
                };
                if let Some(i) = self.fx_index {
                    out.push(Msg::new("/dmx/effect", vec![Arg::S(self.pixel_fx[i].clone()), Arg::F(0.0)]));
                }
                out.push(Msg::new("/dmx/effect", vec![Arg::S(self.pixel_fx[next].clone()), Arg::F(1.0)]));
                self.fx_index = Some(next);
                self.last = format!("LED: {}", self.pixel_fx[next]);
            }
        }
        (out, rumble)
    }

    /// 終わるときに送るもの(押したままのフラッシュと、暗くしたムービングを戻す)
    pub fn release_all(&self) -> Vec<Msg> {
        let mut out = Vec::new();
        if self.movers_dark {
            out.extend(self.movers_dimmer(false));
        }
        if self.prev.contains(&Btn::ZR) {
            out.push(Msg::f("/dmx/flash", 0.0));
        }
        if self.prev.contains(&Btn::ZL) {
            out.push(Msg::f("/dmx/flash/color", 0.0));
        }
        if self.laser {
            out.push(Msg::f("/dmx/laser", 0.0));
        }
        out
    }

    fn random(&mut self) -> f32 {
        // xorshift。色を選ぶだけなので、これで十分
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u64 << 24) as f32
    }
}

const ALL: [Btn; 18] = [
    Btn::A, Btn::B, Btn::X, Btn::Y, Btn::L, Btn::R, Btn::ZL, Btn::ZR, Btn::LStick, Btn::RStick, Btn::Up, Btn::Down, Btn::Left, Btn::Right, Btn::Minus, Btn::Plus, Btn::Capture, Btn::Home,
];

/// 色相(0〜1)と鮮やかさ(0〜1)から RGB(0〜1)。明るさはいつも最大
pub fn hsv(h: f32, s: f32) -> (f32, f32, f32) {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor() as i32 % 6;
    let f = h6 - h6.floor();
    let (p, q, t) = (1.0 - s, 1.0 - s * f, 1.0 - s * (1.0 - f));
    match i {
        0 => (1.0, t, p),
        1 => (q, 1.0, p),
        2 => (p, 1.0, t),
        3 => (p, q, 1.0),
        4 => (t, p, 1.0),
        _ => (1.0, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctl() -> Controller {
        Controller::new(vec![1, 3], vec!["オーロラ".into(), "星空".into(), "レインボー".into()], 42)
    }

    fn held(b: &[Btn]) -> Input {
        Input { held: b.to_vec(), ..Default::default() }
    }

    fn addrs(m: &[Msg]) -> Vec<&str> {
        m.iter().map(|m| m.addr.as_str()).collect()
    }

    #[test]
    fn home_cues_the_laser_while_held_and_keeps_it_alive() {
        let mut c = ctl();
        let (out, _) = c.step(&held(&[Btn::Home]), 0.0);
        assert_eq!(out, vec![Msg::f("/dmx/laser", 1.0)]);
        // 押している間は、dmxctl の期限(1秒)より短い間隔で送り直す
        let mut resent = 0;
        for _ in 0..10 {
            let (out, _) = c.step(&held(&[Btn::Home]), 0.1);
            resent += out.iter().filter(|m| m.addr == "/dmx/laser").count();
        }
        assert!(resent >= 3, "1秒の間に送り直す: {resent}");
        let (out, _) = c.step(&held(&[]), 0.1);
        assert_eq!(out, vec![Msg::f("/dmx/laser", 0.0)]);
        assert!(!c.laser);
    }

    #[test]
    fn quitting_or_pausing_stops_the_laser() {
        let mut c = ctl();
        c.step(&held(&[Btn::Home]), 0.0);
        assert!(c.release_all().contains(&Msg::f("/dmx/laser", 0.0)));
        let (out, _) = c.step(&held(&[Btn::Home, Btn::Plus]), 0.0);
        assert!(out.contains(&Msg::f("/dmx/laser", 0.0)));
        assert!(!c.laser);
    }

    #[test]
    fn right_stick_up_is_red_and_right_is_yellow_green() {
        let mut c = ctl();
        let (out, _) = c.step(&Input { right: (0.0, 1.0), ..Default::default() }, 0.1);
        assert_eq!(c.rgb(), (255, 0, 0));
        assert!(addrs(&out).contains(&"/dmx/main/rgb"));
        // 右(90度) = 色相 0.25 = 黄緑
        c.step(&Input { right: (1.0, 0.0), ..Default::default() }, 0.1);
        assert!((c.hue - 0.25).abs() < 1e-4);
        assert_eq!(c.rgb(), (128, 255, 0));
    }

    #[test]
    fn a_small_tilt_gives_a_pale_color_and_releasing_keeps_it() {
        let mut c = ctl();
        c.step(&Input { right: (0.0, -0.4), ..Default::default() }, 0.1);
        let pale = c.rgb();
        assert!(c.sat > 0.0 && c.sat < 0.5, "{}", c.sat);
        // 離しても色は変わらず、同じ色は送り直さない
        let (out, _) = c.step(&Input::default(), 0.1);
        assert_eq!(c.rgb(), pale);
        assert!(!addrs(&out).contains(&"/dmx/main/rgb"));
    }

    #[test]
    fn nothing_is_sent_until_the_stick_is_used() {
        let mut c = ctl();
        let (out, _) = c.step(&Input::default(), 0.1);
        assert!(!addrs(&out).contains(&"/dmx/main/rgb"));
    }

    #[test]
    fn flash_is_held_only_while_pressed() {
        let mut c = ctl();
        let (on, rumble) = c.step(&held(&[Btn::ZR]), 0.01);
        assert_eq!(on, vec![Msg::f("/dmx/flash", 1.0)]);
        assert!(rumble.is_some());
        let (still, _) = c.step(&held(&[Btn::ZR]), 0.01);
        assert!(still.is_empty());
        let (off, _) = c.step(&Input::default(), 0.01);
        assert_eq!(off, vec![Msg::f("/dmx/flash", 0.0)]);
    }

    #[test]
    fn quitting_while_flashing_releases_the_flash() {
        let mut c = ctl();
        c.step(&held(&[Btn::ZL]), 0.01);
        assert_eq!(c.release_all(), vec![Msg::f("/dmx/flash/color", 0.0)]);
    }

    #[test]
    fn left_stick_moves_and_stays_and_click_recenters() {
        let mut c = ctl();
        let (out, _) = c.step(&Input { left: (1.0, 0.0), ..Default::default() }, 0.5);
        assert!((c.pan - 0.75).abs() < 1e-4);
        assert_eq!(out.iter().filter(|m| m.addr.ends_with("/pan")).count(), 2);
        c.step(&Input::default(), 0.5);
        assert!((c.pan - 0.75).abs() < 1e-4, "離すと止まる");
        c.step(&held(&[Btn::LStick]), 0.1);
        assert_eq!((c.pan, c.tilt), (0.5, 0.5));
    }

    #[test]
    fn starting_up_does_not_move_the_lights() {
        let mut c = ctl();
        let (out, _) = c.step(&Input::default(), 0.1);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn gyro_aims_only_when_turned_on() {
        let mut c = ctl();
        let turn = Input { gyro: Some((0.0, -PI)), ..Default::default() };
        c.step(&turn, 0.25);
        assert_eq!(c.pan, 0.5);
        c.step(&held(&[Btn::Capture]), 0.0);
        c.step(&turn, 0.25);
        assert!((c.pan - 0.75).abs() < 1e-4, "{}", c.pan);
    }

    #[test]
    fn dpad_steps_master_and_cycles_led_effects() {
        let mut c = ctl();
        let (out, _) = c.step(&held(&[Btn::Down]), 0.01);
        assert_eq!(out, vec![Msg::f("/dmx/master", 0.9)]);
        c.step(&Input::default(), 0.01);
        let (first, _) = c.step(&held(&[Btn::Right]), 0.01);
        assert_eq!(first, vec![Msg::new("/dmx/effect", vec![Arg::S("オーロラ".into()), Arg::F(1.0)])]);
        c.step(&Input::default(), 0.01);
        let (back, _) = c.step(&held(&[Btn::Left]), 0.01);
        // 今のを止めて、前のもの(一周して最後)を点ける
        assert_eq!(back[0], Msg::new("/dmx/effect", vec![Arg::S("オーロラ".into()), Arg::F(0.0)]));
        assert_eq!(back[1], Msg::new("/dmx/effect", vec![Arg::S("レインボー".into()), Arg::F(1.0)]));
    }

    #[test]
    fn pause_stops_sending_and_releases_flash() {
        let mut c = ctl();
        let (out, _) = c.step(&held(&[Btn::Plus]), 0.01);
        assert!(c.paused);
        assert!(addrs(&out).contains(&"/dmx/flash"));
        let (none, _) = c.step(&Input { right: (0.0, 1.0), held: vec![Btn::ZR], ..Default::default() }, 0.1);
        assert!(none.is_empty());
    }

    #[test]
    fn minus_returns_to_the_palette() {
        let mut c = ctl();
        c.step(&Input { right: (0.0, 1.0), ..Default::default() }, 0.1);
        let (out, _) = c.step(&held(&[Btn::Minus]), 0.1);
        assert_eq!(addrs(&out), vec!["/dmx/rgb/off"]);
        let (out, _) = c.step(&Input::default(), 0.1);
        assert!(out.is_empty());
    }

    #[test]
    fn r_steps_through_gobos_starting_after_open() {
        let gobos = vec![("オープン".to_string(), 0.0), ("ゴボ1".to_string(), 0.1), ("ゴボ2".to_string(), 0.2)];
        let mut c = ctl().with_mover_info(Vec::new(), gobos);
        let press = |c: &mut Controller| {
            let (out, _) = c.step(&held(&[Btn::R]), 0.01);
            c.step(&Input::default(), 0.01);
            out
        };
        // 最初はゴボ1。2台とも同じゴボにする
        assert_eq!(press(&mut c), vec![Msg::f("/dmx/fixture/1/gobo", 0.1), Msg::f("/dmx/fixture/3/gobo", 0.1)]);
        assert_eq!(press(&mut c)[0], Msg::f("/dmx/fixture/1/gobo", 0.2));
        // 一周してオープンへ
        assert_eq!(press(&mut c)[0], Msg::f("/dmx/fixture/1/gobo", 0.0));
    }

    #[test]
    fn l_darkens_movers_only_while_held_and_restores_their_level() {
        let mut c = ctl().with_mover_info(vec![(1, 0.5)], Vec::new());
        let (dark, _) = c.step(&held(&[Btn::L]), 0.01);
        assert_eq!(dark, vec![Msg::f("/dmx/fixture/1/dimmer", 0.0), Msg::f("/dmx/fixture/3/dimmer", 0.0)]);
        assert_eq!(c.release_all(), vec![Msg::f("/dmx/fixture/1/dimmer", 0.5), Msg::f("/dmx/fixture/3/dimmer", 1.0)]);
        let (back, _) = c.step(&Input::default(), 0.01);
        // 設定から読んだ明るさに戻す。読めなかったムービングは最大に
        assert_eq!(back, vec![Msg::f("/dmx/fixture/1/dimmer", 0.5), Msg::f("/dmx/fixture/3/dimmer", 1.0)]);
        assert!(c.release_all().is_empty());
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv(0.0, 1.0), (1.0, 0.0, 0.0));
        assert_eq!(hsv(1.0 / 3.0, 1.0).1, 1.0);
        assert_eq!(hsv(0.5, 0.0), (1.0, 1.0, 1.0));
    }
}
