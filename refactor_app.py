import re

with open("apps/stillair/cockpit-dioxus/src/app.rs", "r") as f:
    text = f.read()

# Replace imports
text = text.replace(
    "use crate::components::{",
    "use crate::components::sampling_siamese::SamplingSiamese;\nuse crate::components::{"
)

# Replace mfd-bay and move things around
mfd_bay_original = """            // ── MFD bay — 3 equal panels ──────────────────────────────────────
            main {
                id:    "mfd-bay",
                class: "mfd-bay",

                SessionPanel  { mode, session_state, viz_data, show_mastered }
                InsightsPanel { mode, session_state, playback_state, viz_data }
                CoachPanel    { mode, session_state }
            }"""

mfd_bay_new = """            // ── Work Layer — 65% Middle ──────────────────────────────────────
            main {
                id:    "mfd-bay",
                class: "work-layer",

                SamplingSiamese {
                    mode,
                    session_state,
                    playback_state,
                    viz_data,
                    show_mastered: show_mastered.clone(),
                }
            }"""

# Actually, we need to move footer from bottom to top.
# Let's find the footer block
footer_match = re.search(r'(\s*// ── Transport bar .*?footer \{\s*id:\s*"transport-bar".*?\n            \})', text, re.DOTALL)
if footer_match:
    footer_text = footer_match.group(1)
    
    # Remove footer from existing position
    text = text.replace(footer_text, "")
    
    # Insert footer BEFORE mfd-bay
    app_shell_start = text.find('class: "app-shell",')
    if app_shell_start != -1:
        insert_idx = text.find('\n', app_shell_start) + 1
        text = text[:insert_idx] + footer_text + "\n" + text[insert_idx:]

# Replace mfd-bay
text = text.replace(mfd_bay_original, mfd_bay_new)

# Insert Hangar at the end
# find closing brackets of app-shell
if "            // ── MasteredView" in text:
    target_idx = text.rfind("        }")
    hangar_content = """
            // ── Hangar — 25% Bottom ──────────────────────────────────────────
            div { class: "hangar-layer",
                CoachPanel { mode, session_state }
            }
"""
    text = text[:target_idx] + hangar_content + text[target_idx:]

with open("apps/stillair/cockpit-dioxus/src/app.rs", "w") as f:
    f.write(text)

print("Done")
