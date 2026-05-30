import re

with open("apps/stillair/cockpit-dioxus/src/components/transport_bar.rs", "r") as f:
    content = f.read()

# Replace SKIP BACK
content = content.replace(
    '// SKIP BACK\n                                        div { class: "transport-btn-col",\n                                            div { class: "transport-top-label", "–5" }',
    '// SKIP BACK\n                                        if *tier.read() >= crate::types::CockpitTier::Tier2_Medium {\n                                            div { class: "transport-btn-col",\n                                                div { class: "transport-top-label", "–5" }'
)

# We need to find the end of SKIP BACK and close the block.
# Actually, it's easier to just use `multi_replace_file_content` if I do it block by block.
