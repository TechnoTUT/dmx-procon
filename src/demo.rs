//! `--demo`: コントローラーが無くても試せるよう、決まった操作を本物と同じ処理(control.rs)に通して送る。
//! 24秒でひとまわりし、くり返す。
//!
//! | 時間 | 操作 |
//! |---|---|
//! | 0〜8秒 | 右スティックをいっぱいに倒して1周(色相がひとまわり) |
//! | 8〜14秒 | 左スティックで、ムービングを右・左・上・下へ |
//! | 14〜16秒 | ZR を0.5秒ごとに短く押す(白フラッシュ) |
//! | 16〜20秒 | 右スティックを少しだけ倒して回す(淡い色) |
//! | 20〜24秒 | X(ランダムな色)を1秒ごと、十字キーの右(LED のエフェクト)を2秒ごと |

use crate::control::{Btn, Input};
use std::f32::consts::TAU;

pub const LOOP_SECS: f32 = 24.0;

/// 時刻 `t` 秒のときの入力
pub fn input_at(t: f32) -> Input {
    let t = t.rem_euclid(LOOP_SECS);
    let mut i = Input::default();
    // 押しているのは、各区切りのはじめの0.15秒だけ(押して離す)
    let tap = |period: f32, from: f32| (t - from).rem_euclid(period) < 0.15;
    match t {
        t if t < 8.0 => {
            let a = t / 8.0 * TAU;
            i.right = (a.sin(), a.cos());
        }
        t if t < 14.0 => {
            let u = t - 8.0;
            i.left = match u {
                u if u < 1.5 => (1.0, 0.0),
                u if u < 3.0 => (-1.0, 0.0),
                u if u < 4.5 => (0.0, 1.0),
                _ => (0.0, -1.0),
            };
        }
        t if t < 16.0 => {
            if tap(0.5, 14.0) {
                i.held.push(Btn::ZR);
            }
        }
        t if t < 20.0 => {
            let a = (t - 16.0) / 4.0 * TAU;
            i.right = (0.45 * a.sin(), 0.45 * a.cos());
        }
        _ => {
            if tap(1.0, 20.0) {
                i.held.push(Btn::X);
            }
            if tap(2.0, 20.5) {
                i.held.push(Btn::Right);
            }
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Controller;

    #[test]
    fn one_loop_sends_color_moves_flashes_and_effects() {
        let mut c = Controller::new(vec![1], vec!["オーロラ".into(), "星空".into()], 7);
        let mut addrs = std::collections::BTreeSet::new();
        let dt = 1.0 / 120.0;
        let mut t = 0.0;
        while t < LOOP_SECS {
            let (out, _) = c.step(&input_at(t), dt);
            addrs.extend(out.into_iter().map(|m| m.addr));
            t += dt;
        }
        for a in ["/dmx/main/rgb", "/dmx/fixture/1/pan", "/dmx/fixture/1/tilt", "/dmx/flash", "/dmx/effect"] {
            assert!(addrs.contains(a), "{a} が送られていない: {addrs:?}");
        }
        // フラッシュは押しっぱなしで終わらない
        assert!(c.release_all().is_empty());
    }
}
