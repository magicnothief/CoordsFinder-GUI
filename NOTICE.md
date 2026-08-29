# Notice and attribution

## Upstream project

CoordsFinder GUI is a fork of **[CoordsFinder](https://github.com/ALaggyDev/CoordsFinder)**
by **Laggy ([@ALaggyDev](https://github.com/ALaggyDev))**, used under the MIT
Licence. The upstream copyright notice is preserved in [`LICENSE`](./LICENSE),
as the MIT Licence requires.

The search engine — the texture-rotation algorithms, filter compilation, scan
planning, and both the CPU and wgpu backends — is upstream's work. This fork
adds a desktop front-end on top of it and does not modify how the search itself
behaves. Everything under [`coordsfinder/`](./coordsfinder) is upstream code
with one change: `config::load` was split so configuration text already in
memory can be validated without first writing it to a file.

Please credit the original project in any video or write-up that uses this
software, as upstream asks.

Related upstream work:

- [WebCoordsFinder](https://github.com/ALaggyDev/WebCoordsFinder) — the browser
  app for marking up a screenshot and exporting a config.
- [Texture Rotation Reverser](https://github.com/19MisterX98/TextureRotations) by
  19MisterX98, and the
  [Texploit Guide](https://gitea.com/ChromeCrusher/Texploit-Guide) by
  ChromeCrusher, which explain the underlying technique.

## Fork authorship

The GUI fork — everything under [`coordsfinder-gui/`](./coordsfinder-gui), the
documentation in [`docs/`](./docs), and the packaging around them — was
developed by **Claude Opus 5** (Anthropic), working from the direction and
review of [@zselybence](https://github.com/magicnothief), who maintains the
fork and holds copyright in it.

## Third-party dependencies

The binaries statically link their Rust dependencies and embed the default egui
font set. Every one of those licences is permissive, and the full notices —
including the SIL Open Font License and the Ubuntu Font Licence that the
embedded fonts carry, and the Unicode Licence carried by the ICU crates — are
reproduced in [`THIRD-PARTY-NOTICES.md`](./THIRD-PARTY-NOTICES.md).

Where a dependency offers a choice of licences, this project takes the
permissive option; no copyleft terms are relied on or triggered. See that file
for the full list and the reasoning.

## Minecraft

Minecraft is a trademark of Mojang Studios. This project is not affiliated with,
endorsed by, or connected to Mojang Studios or Microsoft.
