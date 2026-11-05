// Album covers: downloaded once on a thread of their own, decoded and kept small. Horizon's
// mods can't upload a texture, so the HUD draws the cover as a grid of filled cells, one per
// screen pixel at the player's GUI scale.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The side of a kept cover, in cells: 40 GUI pixels at GUI scale 2.
pub const SIDE: usize = 80;

#[derive(Clone, Debug, PartialEq)]
pub enum Cover {
    Loading,
    Missing,
    /// `SIDE` x `SIDE` RGB.
    Ready(Arc<Vec<u8>>),
}

#[derive(Default)]
pub struct Covers {
    by_url: Arc<Mutex<HashMap<String, Cover>>>,
    /// The last cover resampled for drawing: (url, side, rgb).
    sized: Mutex<Option<(String, usize, Arc<Vec<u8>>)>>,
}

impl Covers {
    /// The cover at `url`, fetched when first asked for.
    pub fn get(&self, url: &str) -> Cover {
        if url.is_empty() {
            return Cover::Missing;
        }
        let mut map = self.by_url.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = map.get(url) {
            return c.clone();
        }
        // Keep only a few: a long session plays many songs.
        if map.len() > 16 {
            map.retain(|_, c| *c == Cover::Loading);
        }
        map.insert(url.to_owned(), Cover::Loading);
        drop(map);
        let by_url = self.by_url.clone();
        let url = url.to_owned();
        let _ = std::thread::Builder::new().name("spotify-hud-cover".into()).spawn(move || {
            let cover = fetch(&url).map(|rgb| Cover::Ready(Arc::new(rgb))).unwrap_or(Cover::Missing);
            by_url.lock().unwrap_or_else(|e| e.into_inner()).insert(url, cover);
        });
        Cover::Loading
    }

    /// The cover at `url` resampled to `side` x `side`, once per change.
    pub fn sized(&self, url: &str, side: usize) -> Option<Arc<Vec<u8>>> {
        let Cover::Ready(rgb) = self.get(url) else { return None };
        let side = side.clamp(1, SIDE);
        let mut sized = self.sized.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((u, s, px)) = sized.as_ref() {
            if u == url && *s == side {
                return Some(px.clone());
            }
        }
        let px = Arc::new(resample(&rgb, SIDE, SIDE, side));
        *sized = Some((url.to_owned(), side, px.clone()));
        Some(px)
    }
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();
    let res = agent.get(url).call().ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut res.into_reader().take(8 << 20), &mut bytes).ok()?;
    let mut decoder = zune_jpeg::JpegDecoder::new(&bytes);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = pixels.len() / (w * h).max(1);
    if w == 0 || h == 0 || channels == 0 {
        return None;
    }
    let rgb: Vec<u8> = match channels {
        1 => pixels.iter().flat_map(|&g| [g, g, g]).collect(),
        3 => pixels,
        n => pixels.chunks(n).flat_map(|p| [p[0], p[1], p[2]]).collect(),
    };
    // Covers are square; crop the middle of one that isn't.
    let side = w.min(h);
    let (ox, oy) = ((w - side) / 2, (h - side) / 2);
    let mut square = Vec::with_capacity(side * side * 3);
    for y in 0..side {
        let row = ((oy + y) * w + ox) * 3;
        square.extend_from_slice(&rgb[row..row + side * 3]);
    }
    Some(resample(&square, side, side, SIDE))
}

use std::io::Read as _;

/// Box-filters a `w` x `h` RGB image down (or nearest-samples it up) to `side` x `side`.
pub fn resample(rgb: &[u8], w: usize, h: usize, side: usize) -> Vec<u8> {
    let mut out = vec![0u8; side * side * 3];
    for oy in 0..side {
        let y0 = oy * h / side;
        let y1 = ((oy + 1) * h / side).max(y0 + 1).min(h);
        for ox in 0..side {
            let x0 = ox * w / side;
            let x1 = ((ox + 1) * w / side).max(x0 + 1).min(w);
            let mut sum = [0u32; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 3;
                    for c in 0..3 {
                        sum[c] += u32::from(rgb[i + c]);
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            let o = (oy * side + ox) * 3;
            for c in 0..3 {
                out[o + c] = (sum[c] / n) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_averages() {
        // 2x2: red, green / blue, white → 1 pixel of their mean.
        let img = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        assert_eq!(resample(&img, 2, 2, 1), vec![127, 127, 127]);
        assert_eq!(resample(&img, 2, 2, 2), img.to_vec());
        assert_eq!(resample(&img, 2, 2, 4).len(), 48);
    }
}
