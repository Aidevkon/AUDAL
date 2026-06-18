import re
with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

# Add JiniMatrix and LiveData
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

if "struct JiniMatrix" not in content:
    content = structs + content

# Replace the narrative generation block
target = r"""    let \(narrative, action, confidence\) = if behaviour\.quality == QualityBehaviour::Clipping \{
(.*?)
        \)
    \};"""

replacement = """    let persona_str = match persona_id {
        JiniPersonaId::Beginner => "beginner",
        JiniPersonaId::Intermediate => "intermediate",
        JiniPersonaId::Pro => "pro",
    };

    let finding_key = if behaviour.quality == QualityBehaviour::Clipping {
        "Harsh"
    } else if behaviour.loudness == LoudnessBehaviour::TooQuiet {
        "Quiet"
    } else if behaviour.loudness == LoudnessBehaviour::TooLoud {
        "Harsh"
    } else if behaviour.spectral == SpectralBehaviour::Muddy || behaviour.spectral == SpectralBehaviour::Boxy || behaviour.spectral == SpectralBehaviour::Thin {
        "Muddy"
    } else if behaviour.spectral == SpectralBehaviour::Harsh {
        "Harsh"
    } else if behaviour.dynamics == DynamicsBehaviour::Overcompressed || behaviour.dynamics == DynamicsBehaviour::OverCompressed {
        "Flat"
    } else {
        "Perfect"
    };

    let flavour_str = match flavour.as_deref().unwrap_or("Clean") {
        "warm" | "Warm" => "Warm",
        "punch" | "Punch" => "Punch",
        "air" | "Air" => "Air",
        _ => "Clean",
    };

    let platform_str = match platform.as_deref().unwrap_or("Spotify") {
        "apple" | "Apple" | "apple_music" => "Apple",
        "youtube" | "Youtube" | "YouTube" => "Youtube",
        "broadcast" | "Broadcast" => "Broadcast",
        _ => "Spotify",
    };

    let live = LiveData {
        lufs,
        peak: quality.clips_detected as f32,
        platform: platform_str,
        flavour: flavour_str,
        correlation: quality.stereo_correlation,
    };

    let narrative_opt = resolve_narration(finding_key, flavour_str, persona_str, &live);
    
    // Fallback if matrix resolving fails
    let (narrative, action, confidence) = if let Some(n) = narrative_opt {
        // Derive action using the existing if/else logic but without strings
        let act = if behaviour.quality == QualityBehaviour::Clipping {
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, delta: -0.2, reason: "Clipping detected".to_string() })
        } else if behaviour.loudness == LoudnessBehaviour::TooLoud {
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, delta: -0.15, reason: "Loudness exceeds target range".to_string() })
        } else if behaviour.loudness == LoudnessBehaviour::TooQuiet {
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, delta: 0.15, reason: "Loudness below target range".to_string() })
        } else if behaviour.spectral != SpectralBehaviour::Neutral {
            let (to, reason) = match behaviour.spectral {
                SpectralBehaviour::Muddy => (FlavourId::Clean, "Muddy low-end"),
                SpectralBehaviour::Harsh => (FlavourId::Warm, "Harsh upper-mids"),
                SpectralBehaviour::Thin => (FlavourId::Warm, "Thin low-end"),
                SpectralBehaviour::Boxy => (FlavourId::Clean, "Boxy lower-mids"),
                _ => (FlavourId::Clean, "Spectral imbalance"),
            };
            Some(JiniAction::SuggestFlavourSwitch { to, reason: reason.to_string() })
        } else if behaviour.dynamics == DynamicsBehaviour::Overcompressed || behaviour.dynamics == DynamicsBehaviour::OverCompressed {
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Dynamics, delta: -0.1, reason: "Over-compressed dynamic range".to_string() })
        } else if behaviour.stereo == StereoBehaviour::Unstable {
            Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Width, delta: -0.1, reason: "Low stereo correlation".to_string() })
        } else {
            Some(JiniAction::SuggestNothing)
        };
        (n, act, 0.95)
    } else {
        if behaviour.quality == QualityBehaviour::Clipping {
            ("Clipping detected - consider reducing input gain before mastering.".to_string(), Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, delta: -0.2, reason: "Clipping detected".to_string() }), 0.95)
        } else if behaviour.loudness == LoudnessBehaviour::TooLoud {
            ("The mix is quite hot - you might want to bring the loudness down for better dynamics.".to_string(), Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, delta: -0.15, reason: "Loudness exceeds target range".to_string() }), 0.85)
        } else if behaviour.loudness == LoudnessBehaviour::TooQuiet {
            ("The track is very quiet - a small loudness boost would help it compete.".to_string(), Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Loudness, delta: 0.15, reason: "Loudness below target range".to_string() }), 0.80)
        } else if behaviour.spectral != SpectralBehaviour::Neutral {
            let (to, reason) = match behaviour.spectral {
                SpectralBehaviour::Muddy => (FlavourId::Clean, "Muddy low-end"),
                SpectralBehaviour::Harsh => (FlavourId::Warm, "Harsh upper-mids"),
                SpectralBehaviour::Thin => (FlavourId::Warm, "Thin low-end"),
                SpectralBehaviour::Boxy => (FlavourId::Clean, "Boxy lower-mids"),
                _ => (FlavourId::Clean, "Spectral imbalance"),
            };
            (format!("{} detected - switching to {:?} flavour.", reason, to), Some(JiniAction::SuggestFlavourSwitch { to, reason: reason.to_string() }), 0.75)
        } else if behaviour.dynamics == DynamicsBehaviour::Overcompressed || behaviour.dynamics == DynamicsBehaviour::OverCompressed {
            ("The track sounds over-compressed - let's restore some dynamics.".to_string(), Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Dynamics, delta: -0.1, reason: "Over-compressed dynamic range".to_string() }), 0.70)
        } else if behaviour.stereo == StereoBehaviour::Unstable {
            ("Stereo correlation is low - check for phase issues.".to_string(), Some(JiniAction::SuggestMacroChange { handle: MacroHandle::Width, delta: -0.1, reason: "Low stereo correlation".to_string() }), 0.65)
        } else {
            ("Everything looks good - the mix is well-balanced.".to_string(), Some(JiniAction::SuggestNothing), 0.90)
        }
    };"""

content = re.sub(target, replacement, content, flags=re.DOTALL)

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
