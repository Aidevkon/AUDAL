with open("apps/stillair/cockpit-dioxus/src/app.rs", "r") as f:
    text = f.read()

# Replace imports
text = text.replace(
    "use crate::components::{\n    screw::Screw,",
    "use crate::components::sampling_siamese::SamplingSiamese;\nuse crate::components::{\n    screw::Screw,"
)

mfd_start = text.find("            // ── MFD bay")
mfd_end = text.find("            // ── MasteredView")

# Transport bar starts at 
tb_start = text.find("            // ── Transport bar")
tb_end = text.find("        }\n    }\n}")

if mfd_start == -1 or tb_start == -1:
    print("Could not find sections!")
    exit(1)

mfd_content = text[mfd_start:mfd_end]
tb_content = text[tb_start:tb_end]

# New MFD content
new_mfd = """            // ── Work Layer — 65% Middle ──────────────────────────────────────
            main {
                id:    "mfd-bay",
                class: "mfd-bay work-layer",

                SamplingSiamese {
                    mode,
                    session_state,
                    playback_state,
                    viz_data,
                    show_mastered: show_mastered.clone(),
                }
            }

"""

hangar_content = """            // ── Hangar — 25% Bottom ──────────────────────────────────────────
            div { class: "hangar-layer coach-panel",
                CoachPanel { mode, session_state }
            }
"""

new_body = tb_content + "\n" + new_mfd + text[mfd_end:tb_start] + hangar_content

final_text = text[:mfd_start] + new_body + "        }\n    }\n}\n"

with open("apps/stillair/cockpit-dioxus/src/app.rs", "w") as f:
    f.write(final_text)

print("Done")
