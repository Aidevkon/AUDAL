# THIRD-PARTY EMBEDDED ASSETS

Scope: assets embedded in shipped binaries via `include_bytes!`.
(Cargo crate dependencies carry their own licenses via crates.io
metadata and are not listed here. Trained model asset provenance is
tracked separately in `FINDINGS.md` F-066.)

## Courier Prime (font)

- File: `lineos/m0/m0-daemon/src/assets/fonts/CourierPrime-Regular.ttf`
- Embedded in: `m0d` (certificate PNG generation, `png_gen.rs`)
- Copyright: © 2015 Quote-Unquote Apps (design: Alan Dague-Greene)
- License: SIL Open Font License, Version 1.1 (OFL-1.1)
- License text: https://openfontlicense.org — the font is embedded
  unmodified; the reserved font name "Courier Prime" is not used to
  name any derivative.
