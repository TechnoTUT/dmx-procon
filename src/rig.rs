//! dmxctl の設定(project.json と機材定義)を読んで、操作する相手を決める。
//! - ムービング: パン・チルトがあり、ARM の要らない機材(レーザーは動かさない)。
//!   L で暗くしたあとに戻す明るさと、R で切り替えるゴボも、ここで読む
//! - LED バーのエフェクト: 十字キーの左右で順に切り替える

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// dmxctl の設定フォルダ。dmxctl と同じく DMXCTL_CONFIG_DIR で変えられる
pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("DMXCTL_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return d.into();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/dmxctl")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("dmxctl")
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config")).join("dmxctl")
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Rig {
    /// パン・チルトを動かす機材のID(dmxctl のパッチタブの番号)
    pub movers: Vec<u32>,
    /// ムービングの、dmxctl の設定に入っている明るさ(0〜1)。L で暗くしたあと、離したらこの値に戻す
    pub dimmers: Vec<(u32, f32)>,
    /// R で順に切り替えるゴボ(名前と、送る値 0〜1)。最初のムービングの機材定義から読む
    pub gobos: Vec<(String, f32)>,
    /// LED バーのエフェクトの名前(十字キーの左右で切り替える順)
    pub pixel_fx: Vec<String>,
}

/// dmxctl に内蔵の機材定義のうち、パン・チルトがあって ARM の要らないもの。
/// 使うところ(チャンネルの並びとゴボ)だけを、dmxctl の fixtures/stage-evolution_minispot30.json から写してある
const BUILTIN_MOVERS: &str = r#"[{
  "id": "stage-evolution/minispot30",
  "modes": [
    {"channels": [{"attr":"pan"},{"attr":"tilt"},{"attr":"color"},{"attr":"gobo","ranges":[
      {"from":0,"to":7,"label":"オープン"},{"from":8,"to":15,"label":"ゴボ1"},{"from":16,"to":24,"label":"ゴボ2"},
      {"from":25,"to":31,"label":"ゴボ3"},{"from":32,"to":39,"label":"ゴボ4"},{"from":40,"to":47,"label":"ゴボ5"},
      {"from":48,"to":55,"label":"ゴボ6"},{"from":56,"to":63,"label":"ゴボ7"},{"from":64,"to":71,"label":"オープン(2)"},
      {"from":72,"to":127,"label":"ゴボ シェイク","sweep":true},{"from":128,"to":255,"label":"ゴボホイール回転","sweep":true}]},
      {"attr":"strobe"},{"attr":"dimmer"},{"attr":"pt_speed"},{"attr":"auto"},{"attr":"control"}]},
    {"channels": [{"attr":"pan"},{"attr":"pan_fine"},{"attr":"tilt"},{"attr":"tilt_fine"},{"attr":"color"},{"attr":"gobo"},
      {"attr":"strobe"},{"attr":"dimmer"},{"attr":"pt_speed"},{"attr":"auto"},{"attr":"control"}]}
  ]
}]"#;

impl Rig {
    pub fn load(dir: &Path) -> Result<Rig, String> {
        let path = dir.join("project.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        let project: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        Ok(Rig::from_project(&project, &mover_profiles(&dir.join("fixtures"))))
    }

    /// `profiles` は、ムービングの機材定義(ID → 定義)
    pub fn from_project(project: &Value, profiles: &HashMap<String, Value>) -> Rig {
        let fixtures = project["fixtures"].as_array().map(Vec::as_slice).unwrap_or_default();
        let mut rig = Rig::default();
        for f in fixtures {
            let (Some(id), Some(profile)) = (f["id"].as_u64(), f["profile"].as_str().and_then(|p| profiles.get(p))) else { continue };
            let id = id as u32;
            rig.movers.push(id);
            let mode = &profile["modes"][f["mode"].as_u64().unwrap_or(0) as usize];
            let attrs: Vec<&str> = mode["channels"].as_array().into_iter().flatten().map(|c| c["attr"].as_str().unwrap_or("")).collect();
            if let Some(v) = attrs.iter().position(|&a| a == "dimmer").and_then(|i| f["values"][i].as_u64()) {
                rig.dimmers.push((id, v as f32 / 255.0));
            }
            if rig.gobos.is_empty() {
                rig.gobos = gobos(mode);
            }
        }
        rig.pixel_fx = project["pixel_fx"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(|e| e["name"].as_str().map(String::from)).collect();
        rig
    }
}

/// ゴボのチャンネルの、止まったゴボの範囲(回転・シェイクは除く)。同じものの2つ目(「オープン(2)」など)も除く
fn gobos(mode: &Value) -> Vec<(String, f32)> {
    let ch = mode["channels"].as_array().into_iter().flatten().find(|c| c["attr"] == "gobo");
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in ch.and_then(|c| c["ranges"].as_array()).into_iter().flatten() {
        let (Some(from), Some(to), Some(label)) = (r["from"].as_u64(), r["to"].as_u64(), r["label"].as_str()) else { continue };
        let base = label.split(['(', '(']).next().unwrap_or(label).trim().to_string();
        if r["sweep"] == true || !seen.insert(base) {
            continue;
        }
        out.push((label.to_string(), (from + to) as f32 / 2.0 / 255.0));
    }
    out
}

/// ムービングの機材定義(ID → 定義)。内蔵のものと、利用者の機材定義(fixtures/*.json)のうちパン・チルトがあって ARM の要らないもの
fn mover_profiles(dir: &Path) -> HashMap<String, Value> {
    let mut out: HashMap<String, Value> = HashMap::new();
    let builtin: Vec<Value> = serde_json::from_str(BUILTIN_MOVERS).unwrap_or_default();
    let users = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok());
    for p in builtin.into_iter().chain(users) {
        if is_mover(&p)
            && let Some(id) = p["id"].as_str()
        {
            out.insert(id.to_string(), p);
        }
    }
    out
}

fn is_mover(profile: &Value) -> bool {
    if profile["needs_arm"].as_bool() == Some(true) {
        return false;
    }
    let attrs: HashSet<&str> = profile["modes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|m| m["channels"].as_array().into_iter().flatten())
        .filter_map(|c| c["attr"].as_str())
        .collect();
    attrs.contains("pan") && attrs.contains("tilt")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_movers_their_dimmer_and_gobos_and_skips_lasers() {
        let project = json!({
            "fixtures": [
                {"id": 1, "profile": "stage-evolution/minispot30", "mode": 0, "values": [127, 127, 54, 75, 0, 255, 0, 0, 0]},
                {"id": 9, "profile": "stage-evolution/laserscan300"},
                {"id": 6, "profile": "user/twinkle_ball"},
                {"id": 3, "profile": "stage-evolution/minispot30", "mode": 0, "values": [127, 127, 54, 75, 0, 51, 0, 0, 0]}
            ],
            "pixel_fx": [{"name": "オーロラ"}, {"name": "星空"}]
        });
        let rig = Rig::from_project(&project, &mover_profiles(Path::new("/nonexistent")));
        assert_eq!(rig.movers, vec![1, 3]);
        assert_eq!(rig.dimmers, vec![(1, 1.0), (3, 0.2)]);
        let names: Vec<&str> = rig.gobos.iter().map(|g| g.0.as_str()).collect();
        assert_eq!(names, ["オープン", "ゴボ1", "ゴボ2", "ゴボ3", "ゴボ4", "ゴボ5", "ゴボ6", "ゴボ7"]);
        // ゴボ1(8〜15)の真ん中
        assert!((rig.gobos[1].1 * 255.0 - 11.5).abs() < 1e-3);
        assert_eq!(rig.pixel_fx, vec!["オーロラ", "星空"]);
    }

    #[test]
    fn a_profile_that_needs_arm_is_not_a_mover() {
        let ch = |a: &str| json!({"attr": a});
        let head = json!({"id": "x", "modes": [{"channels": [ch("pan"), ch("tilt")]}]});
        let laser = json!({"id": "y", "needs_arm": true, "modes": [{"channels": [ch("pan"), ch("tilt")]}]});
        let par = json!({"id": "z", "modes": [{"channels": [ch("red")]}]});
        assert!(is_mover(&head));
        assert!(!is_mover(&laser));
        assert!(!is_mover(&par));
    }
}
