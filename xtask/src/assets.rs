//! Every asset file (image, icon, font, sound, video) needs a row in ATTRIBUTION.md naming its
//! author, source and licence. GitCraft draws its icons in code, so today there are none.

use std::path::Path;

const ASSET_EXT: &[&str] =
    &["png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "icns", "bmp", "ttf", "otf", "woff", "woff2", "wav", "mp3", "ogg", "mp4"];

pub fn run(root: &Path) -> Result<(), String> {
    let attribution = std::fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut missing = Vec::new();
    let mut count = 0;
    crate::walk(root, &mut |p| {
        let ext = p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if ASSET_EXT.contains(&ext.as_str()) {
            count += 1;
            let rel = p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/");
            if !attribution.contains(&format!("`{rel}`")) {
                missing.push(rel);
            }
        }
    });
    if missing.is_empty() {
        println!("assets: OK ({count} files attributed)");
        Ok(())
    } else {
        Err(format!("assets without a row in ATTRIBUTION.md:\n  {}", missing.join("\n  ")))
    }
}
