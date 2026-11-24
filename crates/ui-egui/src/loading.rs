//! Animated loading pictures: GIFs bundled under `assets/`, decoded once into textures and played
//! in a loop. A picture that fails to decode falls back to egui's spinner.

use std::sync::Arc;

use egui::{ColorImage, TextureHandle, TextureOptions, vec2};
use image::AnimationDecoder;

/// Radar's loading picture (`assets/radar.gif`).
pub const RADAR_GIF: &[u8] = include_bytes!("../../../assets/radar.gif");

/// Most frames kept, and largest side, of a bundled animation.
const MAX_FRAMES: usize = 240;
const MAX_SIDE: u32 = 2048;
/// Shortest frame time, in milliseconds (GIFs with 0 or tiny delays would spin the CPU).
const MIN_DELAY_MS: u32 = 20;

/// A decoded animation: its frames, each with how long it stays, in milliseconds.
pub struct Animation {
    frames: Vec<(TextureHandle, u32)>,
    total_ms: u64,
    size: egui::Vec2,
}

/// The frames of a GIF, composited to full RGBA, with their delays; `None` when it does not decode.
pub fn decode_gif(bytes: &[u8]) -> Option<Vec<(ColorImage, u32)>> {
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let mut out = Vec::new();
    for frame in decoder.into_frames().take(MAX_FRAMES) {
        let frame = frame.ok()?;
        let (num, den) = frame.delay().numer_denom_ms();
        let ms = num.checked_div(den).unwrap_or(0).max(MIN_DELAY_MS);
        let buf = frame.into_buffer();
        if buf.width() == 0 || buf.height() == 0 || buf.width() > MAX_SIDE || buf.height() > MAX_SIDE {
            return None;
        }
        let size = [buf.width() as usize, buf.height() as usize];
        out.push((ColorImage::from_rgba_unmultiplied(size, buf.as_raw()), ms));
    }
    (!out.is_empty()).then_some(out)
}

/// The animation `name` made from `bytes`, decoded and uploaded on first use and kept in the context.
fn animation(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<Arc<Animation>> {
    let id = egui::Id::new(("loading-animation", name));
    if let Some(kept) = ctx.data(|d| d.get_temp::<Option<Arc<Animation>>>(id)) {
        return kept;
    }
    let anim = decode_gif(bytes).map(|frames| {
        let size = frames.first().map(|(img, _)| vec2(img.size[0] as f32, img.size[1] as f32)).unwrap_or_default();
        let total_ms = frames.iter().map(|(_, ms)| u64::from(*ms)).sum();
        let frames =
            frames.into_iter().enumerate().map(|(i, (img, ms))| (ctx.load_texture(format!("{name}:{i}"), img, TextureOptions::LINEAR), ms)).collect();
        Arc::new(Animation { frames, total_ms, size })
    });
    ctx.data_mut(|d| d.insert_temp(id, anim.clone()));
    anim
}

/// Plays the animation `name` from `bytes`, at most `max_width` wide; a spinner when it cannot.
pub fn show(ui: &mut egui::Ui, name: &str, bytes: &[u8], max_width: f32) {
    let Some(anim) = animation(ui.ctx(), name, bytes).filter(|a| a.total_ms > 0) else {
        ui.add(egui::Spinner::new());
        return;
    };
    let now_ms = (ui.input(|i| i.time) * 1000.0).max(0.0) as u64;
    let mut t = now_ms % anim.total_ms;
    let mut current = anim.frames.first();
    let mut left = 0u64;
    for f in &anim.frames {
        let ms = u64::from(f.1);
        if t < ms {
            current = Some(f);
            left = ms.saturating_sub(t);
            break;
        }
        t = t.saturating_sub(ms);
    }
    let Some((tex, _)) = current else {
        ui.add(egui::Spinner::new());
        return;
    };
    let scale = if anim.size.x > max_width && anim.size.x > 0.0 { max_width / anim.size.x } else { 1.0 };
    ui.add(egui::Image::new((tex.id(), anim.size * scale)).corner_radius(6.0));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(left.max(1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radar_gif_decodes() {
        let frames = decode_gif(RADAR_GIF).unwrap();
        assert!(frames.len() > 1);
        assert!(frames.iter().all(|(_, ms)| *ms >= MIN_DELAY_MS));
    }

    #[test]
    fn junk_never_panics() {
        for junk in [&b""[..], b"GIF89a", b"not a gif", &RADAR_GIF[..RADAR_GIF.len().min(100)]] {
            let _ = decode_gif(junk);
        }
    }
}
