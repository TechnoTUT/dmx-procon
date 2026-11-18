//! dmxctl の設定(project.json と機材定義)を読んで、操作する相手を決める。
//! - ムービング: パン・チルトがあり、ARM の要らない機材(レーザーは動かさない)
//! - LED バーのエフェクト: 十字キーの左右で順に切り替える

use serde_json::Value;
use std::collections::HashSet;
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
    /// LED バーのエフェクトの名前(十字キーの左右で切り替える順)
    pub pixel_fx: Vec<String>,
}

/// dmxctl に内蔵の機材定義のうち、パン・チルトがあって ARM の要らないもの
const BUILTIN_MOVERS: &[&str] = &["stage-evolution/minispot30"];

impl Rig {
    pub fn load(dir: &Path) -> Result<Rig, String> {
        let path = dir.join("project.json");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        let project: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{} を読めません: {e}", path.display()))?;
        let movers_profiles = mover_profiles(&dir.join("fixtures"));
        Ok(Rig::from_project(&project, &movers_profiles))
    }

    pub fn from_project(project: &Value, mover_profiles: &HashSet<String>) -> Rig {
        let fixtures = project["fixtures"].as_array().map(Vec::as_slice).unwrap_or_default();
        let movers = fixtures
            .iter()
            .filter(|f| f["profile"].as_str().is_some_and(|p| BUILTIN_MOVERS.contains(&p) || mover_profiles.contains(p)))
            .filter_map(|f| f["id"].as_u64().map(|id| id as u32))
            .collect();
        let pixel_fx = project["pixel_fx"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(|e| e["name"].as_str().map(String::from)).collect();
        Rig { movers, pixel_fx }
    }
}

/// 利用者の機材定義(fixtures/*.json)のうち、パン・チルトがあって ARM の要らないもののID
fn mover_profiles(dir: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let Ok(p) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else { continue };
        if is_mover(&p) {
            if let Some(id) = p["id"].as_str() {
                out.insert(id.to_string());
            }
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
    fn finds_movers_and_skips_lasers() {
        let project = json!({
            "fixtures": [
                {"id": 1, "profile": "stage-evolution/minispot30"},
                {"id": 9, "profile": "stage-evolution/laserscan300"},
                {"id": 6, "profile": "user/twinkle_ball"},
                {"id": 30, "profile": "user/head"}
            ],
            "pixel_fx": [{"name": "オーロラ"}, {"name": "星空"}]
        });
        let users: HashSet<String> = ["user/head".to_string()].into();
        let rig = Rig::from_project(&project, &users);
        assert_eq!(rig.movers, vec![1, 30]);
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
