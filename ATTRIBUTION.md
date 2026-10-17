# Attribution

Every asset file in this repository (image, icon, font, sound, video) has a row here: its path, author,
source and licence. `cargo xtask assets` (part of `cargo xtask ci`) fails when one is missing.

GitCraft's icons and app mark are drawn in code (`crates/ui-egui/src/icons.rs`,
`crates/ui-egui/src/start.rs`): original work by the GitCraft
contributors, under the project's licence. The interface uses the fonts egui bundles by default
(Ubuntu Light, Hack, Noto Emoji, egui's emoji icon font) through the `egui` crate, under their own
open licences.

| File | Author | Source | Licence |
|---|---|---|---|
| `assets/gitcraft.png` (application and window icon) | GitCraft contributors | Original artwork made for this project | MIT OR Apache-2.0 |
