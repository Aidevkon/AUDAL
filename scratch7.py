with open('apps/stillair/src-tauri/src/commands/session.rs', 'r') as f:
    content = f.read()

content = content.replace("crate::types::ZoneFlagsJson::default()", "ZoneFlagsJson::default()")

with open('apps/stillair/src-tauri/src/commands/session.rs', 'w') as f:
    f.write(content)
