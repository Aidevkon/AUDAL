with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

import re

new_structs = """use std::collections::HashMap;

#[derive(serde::Deserialize)]
struct JiniMatrix {
    zone_a: HashMap<String, HashMap<String, Vec<String>>>,
    zone_b: HashMap<String, HashMap<String, HashMap<String, Vec<String>>>>,
    zone_c: HashMap<String, HashMap<String, Vec<String>>>,
}

struct LiveData<'a> {
    lufs: f32,
    peak: f32,
    platform: &'a str,
    flavour: &'a str,
    correlation: f32,
}

fn resolve_narration(
    zone: &str,
    stage: &str,
    finding: &str,
    flavour: &str,
    platform: &str,
    persona: &str,
    live: &LiveData,
) -> Option<String> {
    let asset_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("assets/jini_matrix.json")))
        .unwrap_or_else(|| std::path::Path::new("assets/jini_matrix.json").to_path_buf());

    let content = match std::fs::read_to_string(&asset_path) {
        Ok(c) => c,
        Err(_) => include_str!("../../../../../lineos/m0/m0-daemon/src/assets/jini_matrix.json").to_string(),
    };
    
    let matrix: JiniMatrix = serde_json::from_str(&content).ok()?;
    
    let variations = match zone {
        "zone_a" => matrix.zone_a.get(stage)?.get(persona)?,
        "zone_b" => matrix.zone_b.get(finding)?.get(flavour)?.get(persona)?,
        "zone_c" => matrix.zone_c.get(platform)?.get(persona)?,
        _ => return None,
    };
    
    if variations.is_empty() {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let choice = &variations[(now as usize) % variations.len()];

    let resolved = choice
        .replace("{track_name}", "your track")
        .replace("{bpm}", "120")
        .replace("{lufs}", &format!("{:.1}", live.lufs))
        .replace("{peak}", &format!("{:.1}", live.peak))
        .replace("{correlation}", &format!("{:.2}", live.correlation))
        .replace("{platform}", live.platform)
        .replace("{flavour}", live.flavour);

    Some(resolved)
}
"""

old_structs_pattern = re.compile(r'use std::collections::HashMap;.*?Some\(resolved\)\n\}\n', re.DOTALL)
content = old_structs_pattern.sub(new_structs, content)

old_call = "resolve_narration(finding_key, flavour_str, persona_str, &live)"
new_call = 'resolve_narration("zone_b", "", finding_key, flavour_str, platform_str, persona_str, &live)'
content = content.replace(old_call, new_call)

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
