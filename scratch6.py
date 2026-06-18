with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

content = content.replace("lineos_types::ZoneFlagsJson::default()", "crate::types::ZoneFlagsJson::default()")

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
