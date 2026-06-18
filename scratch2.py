with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

# The structs were injected at the very top.
# Let's remove them and put them after the `use` statements.
structs = """use std::collections::HashMap;

#[derive(serde::Deserialize)]
struct JiniMatrix {
    zone_b: HashMap<String, HashMap<String, HashMap<String, Vec<String>>>>,
}

struct LiveData<'a> {
    lufs: f32,
    peak: f32,
    platform: &'a str,
    flavour: &'a str,
    correlation: f32,
}

fn resolve_narration(
    finding: &str,
    flavour: &str,
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
    let variations = matrix.zone_b.get(finding)?.get(flavour)?.get(persona)?;
    
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

if content.startswith("use std::collections::HashMap;"):
    content = content[len(structs):]

import re
# Insert structs after the first batch of imports
content = re.sub(r'(use serde::\{Deserialize, Serialize\};)', r'\1\n\n' + structs, content)

# Fix the persona_id move error
content = content.replace("let _persona = persona_id;", "let _persona = persona_id.clone();")

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
