with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

test = """
    #[test]
    fn test_matrix_resolution() {
        let quality = make_quality();
        let zones = lineos_types::ZoneFlagsJson::default();
        let s = build_jini_suggestion(
            &quality,
            &zones,
            -14.0,
            lineos_types::JiniPersonaId::Intermediate,
            Some("Warm".to_string()),
            Some("Spotify".to_string()),
        );
        eprintln!("Test result: {:?}", s.narrative);
    }
"""

if "fn test_matrix_resolution" not in content:
    content = content.replace("mod tests {", "mod tests {\n" + test)

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
