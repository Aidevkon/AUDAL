import re
with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

target = r"""    let asset_path = std::env::current_exe()
        \.ok\(\)
        \.and_then\(\|p\| p\.parent\(\)\.map\(\|p\| p\.join\("assets/jini_matrix\.json"\)\)\)
        \.unwrap_or_else\(\|\| std::path::Path::new\("assets/jini_matrix\.json"\)\.to_path_buf\(\)\);

    let content = match std::fs::read_to_string\(&asset_path\) \{
        Ok\(c\) => c,
        Err\(_\) => include_str!\("\.\./\.\./\.\./\.\./\.\./lineos/m0/m0-daemon/src/assets/jini_matrix\.json"\)\.to_string\(\),
    \};
    
    let matrix: JiniMatrix = serde_json::from_str\(&content\)\.ok\(\)\?;"""

replacement = """    let asset_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("assets/jini_matrix.json")))
        .unwrap_or_else(|| std::path::Path::new("assets/jini_matrix.json").to_path_buf());

    eprintln!("[MATRIX] trying path: {:?}", asset_path);
    let content = match std::fs::read_to_string(&asset_path) {
        Ok(c) => {
            eprintln!("[MATRIX] file read OK, {} bytes", c.len());
            match serde_json::from_str::<JiniMatrix>(&c) {
                Ok(_) => eprintln!("[MATRIX] parse OK"),
                Err(e) => eprintln!("[MATRIX] PARSE ERROR: {}", e),
            }
            c
        }
        Err(e) => {
            eprintln!("[MATRIX] FILE READ ERROR: {} (path: {:?})", e, asset_path);
            let fallback = include_str!("../../../../../lineos/m0/m0-daemon/src/assets/jini_matrix.json").to_string();
            eprintln!("[MATRIX] Using include_str! fallback. Len: {}", fallback.len());
            fallback
        }
    };
    
    eprintln!("[MATRIX] lookup: zone={} stage={} finding={} flavour={} persona={}", zone, stage, finding, flavour, persona);
    
    let matrix: JiniMatrix = match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[MATRIX] PARSE ERROR in fallback/content: {}", e);
            return None;
        }
    };"""

content = re.sub(target, replacement, content)

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
