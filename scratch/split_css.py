import os
import re

css_path = 'apps/stillair/cockpit-dioxus/assets/styles.css'
with open(css_path, 'r') as f:
    content = f.read()

def get_block(start_marker, end_marker):
    start_idx = content.find(start_marker)
    if start_idx == -1:
        raise Exception(f"Start marker not found: {start_marker[:30]}")
    if end_marker is None:
        return content[start_idx:], ""
    end_idx = content.find(end_marker, start_idx)
    if end_idx == -1:
        raise Exception(f"End marker not found: {end_marker[:30]}")
    return content[start_idx:end_idx], content[:start_idx] + content[end_idx:]

# 1. tokens.css: ONLY the :root {} block
tokens_start = content.find(":root {")
tokens_end = content.find("}", tokens_start) + 1
tokens_css = content[tokens_start:tokens_end] + "\n"

# 2. base.css: *, html/body reset, .app-shell/#main rules
base_start1 = content.find("/*\n * styles.css")
base_end1 = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   1. Design Tokens")
base_part1 = content[base_start1:base_end1]

base_start2 = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   2. Reset & Root")
base_end2 = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   3. Header / Mode Bar")
base_part2 = content[base_start2:base_end2]
base_css = base_part1 + base_part2

# 3. panels.css: sections 5-11
panels_start = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   5. Panel Screw Wrapper")
panels_end = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   12. Spectrum")
panels_css = content[panels_start:panels_end]

# 4. transport.css: section 18, Task 3.5, Task 3.6, Transport Bar 1:1 Overhaul
# Section 18
tr_18_start = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   18. Transport Bar")
tr_18_end = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   19. Drop Zone")
tr_18 = content[tr_18_start:tr_18_end]

# Task 3.5 onwards (includes Task 3.6, Mockup Overhaul, ABORT ZONE)
# Let's find where Task 3.5 starts.
t35_start = content.find("/* =========================================================================\n   Task 3.5: Annunciators")
# It goes to the end of the file.
t35_css = content[t35_start:]
transport_css = tr_18 + t35_css

# 5. components.css: Everything else.
# Build it by concatenating what's left.
# What's left: sections 3, 4, 12-17, 19-30
c_3_4_start = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   3. Header / Mode Bar")
c_3_4_end = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   5. Panel Screw Wrapper")
c_3_4 = content[c_3_4_start:c_3_4_end]

c_12_17_start = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   12. Spectrum")
c_12_17_end = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   18. Transport Bar")
c_12_17 = content[c_12_17_start:c_12_17_end]

c_19_30_start = content.find("/* ═══════════════════════════════════════════════════════════════════════════════\n   19. Drop Zone")
c_19_30_end = content.find("/* =========================================================================\n   Task 3.5: Annunciators")
c_19_30 = content[c_19_30_start:c_19_30_end]

components_css = c_3_4 + c_12_17 + c_19_30

out_dir = 'apps/stillair/cockpit-dioxus/assets'
with open(os.path.join(out_dir, 'tokens.css'), 'w') as f: f.write(tokens_css)
with open(os.path.join(out_dir, 'base.css'), 'w') as f: f.write(base_css)
with open(os.path.join(out_dir, 'panels.css'), 'w') as f: f.write(panels_css)
with open(os.path.join(out_dir, 'transport.css'), 'w') as f: f.write(transport_css)
with open(os.path.join(out_dir, 'components.css'), 'w') as f: f.write(components_css)

print("Split complete!")
