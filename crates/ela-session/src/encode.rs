//! Lossy encoding of large framebuffer updates. Moving bytes into the webview is what
//! limits full-screen frames (ADR-0001), so large areas go as JPEG while they change. The
//! pump sends them again losslessly once they settle.

use std::borrow::Cow;

use jpeg_encoder::{ColorType, Encoder};

use crate::Rect;
use crate::wire::{self, Encoding};

const JPEG_QUALITY: u8 = 75;
const BANDS: u16 = 4;
/// Below this, splitting a rect costs more than it saves.
const MIN_BAND_HEIGHT: u16 = 128;

pub(crate) fn encode_lossy(raw: &[u8], lossy: &[Rect]) -> Vec<u8> {
    let Some(frame) = wire::parse_frame(raw) else {
        return raw.to_vec();
    };
    let mut pieces: Vec<(Rect, Encoding, Cow<[u8]>)> = Vec::new();
    for r in &frame.rects {
        match lossy
            .contains(&r.rect)
            .then(|| jpeg_bands(r.data, r.rect))
            .flatten()
        {
            Some(bands) => {
                pieces.extend(
                    bands
                        .into_iter()
                        .map(|(band, data)| (band, Encoding::Jpeg, Cow::Owned(data))),
                );
            }
            None => pieces.push((r.rect, r.encoding, Cow::Borrowed(r.data))),
        }
    }
    let entries: Vec<_> = pieces
        .iter()
        .map(|(rect, encoding, data)| (*rect, *encoding, data.len()))
        .collect();
    let mut msg = wire::frame_header(frame.surface_w, frame.surface_h, &entries);
    for (_, _, data) in &pieces {
        msg.extend_from_slice(data);
    }
    msg
}

/// Splits a rect into horizontal bands to encode in parallel. Heights are multiples of 16,
/// the JPEG block size with chroma subsampling, so no band ends in a partial block; only
/// the last one, against the rect's own edge, may.
fn bands(rect: Rect) -> Vec<Rect> {
    let height = (rect.h / BANDS) & !15;
    if height < MIN_BAND_HEIGHT {
        return vec![rect];
    }
    (0..BANDS)
        .map(|i| {
            let h = if i == BANDS - 1 {
                rect.h - height * (BANDS - 1)
            } else {
                height
            };
            Rect::new(rect.x, rect.y + height * i, rect.w, h)
        })
        .collect()
}

fn jpeg_bands(rgba: &[u8], rect: Rect) -> Option<Vec<(Rect, Vec<u8>)>> {
    let stride = usize::from(rect.w) * 4;
    std::thread::scope(|scope| {
        let workers: Vec<_> = bands(rect)
            .into_iter()
            .map(|band| {
                let start = usize::from(band.y - rect.y) * stride;
                let pixels = &rgba[start..start + usize::from(band.h) * stride];
                scope.spawn(move || jpeg(pixels, band).map(|data| (band, data)))
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().ok().flatten())
            .collect()
    })
}

fn jpeg(rgba: &[u8], rect: Rect) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(rgba.len() / 8);
    Encoder::new(&mut out, JPEG_QUALITY)
        .encode(rgba, rect.w, rect.h, ColorType::Rgba)
        .ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth gradient, so JPEG keeps it close to the original.
    fn gradient(w: u16, h: u16) -> Vec<u8> {
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[(x * 2) as u8, (y * 2) as u8, 128, 255]);
            }
        }
        px
    }

    #[test]
    fn only_the_lossy_rects_become_jpeg() {
        let (large, small) = (Rect::new(0, 0, 64, 64), Rect::new(70, 0, 2, 2));
        let (large_px, small_px) = (gradient(64, 64), gradient(2, 2));
        let entries = [
            (large, Encoding::Raw, large_px.len()),
            (small, Encoding::Raw, small_px.len()),
        ];
        let mut raw = wire::frame_header(100, 100, &entries);
        raw.extend_from_slice(&large_px);
        raw.extend_from_slice(&small_px);

        let encoded = encode_lossy(&raw, &[large]);
        assert!(encoded.len() < raw.len());
        let frame = wire::parse_frame(&encoded).unwrap();
        assert_eq!(frame.rects[0].encoding, Encoding::Jpeg);
        assert_eq!(frame.rects[1].encoding, Encoding::Raw);
        assert_eq!(frame.rects[1].data, &small_px[..]);

        let mut decoder = jpeg_decoder::Decoder::new(frame.rects[0].data);
        let rgb = decoder.decode().unwrap();
        let info = decoder.info().unwrap();
        assert_eq!((info.width, info.height), (64, 64));
        let furthest = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .zip(large_px.as_chunks::<4>().0)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)))
            .max()
            .unwrap();
        assert!(furthest < 16, "decoded pixels are off by {furthest}");
    }

    #[test]
    fn tall_rects_split_into_bands_that_cover_them_exactly() {
        let rect = Rect::new(10, 20, 64, 1080);
        let parts = bands(rect);
        assert_eq!(parts.len(), usize::from(BANDS));
        let mut next_y = rect.y;
        for part in &parts {
            assert_eq!((part.x, part.y, part.w), (rect.x, next_y, rect.w));
            next_y += part.h;
        }
        assert_eq!(next_y, rect.y + rect.h);
        assert!(parts[..parts.len() - 1].iter().all(|p| p.h % 16 == 0));
        assert_eq!(bands(Rect::new(0, 0, 64, 64)), [Rect::new(0, 0, 64, 64)]);
    }

    #[test]
    fn every_band_decodes_to_its_own_part_of_the_rect() {
        let rect = Rect::new(0, 0, 32, 1024);
        let mut px = Vec::new();
        for y in 0..rect.h {
            for _ in 0..rect.w {
                px.extend_from_slice(&[(y / 8) as u8, 40, 200, 255]);
            }
        }
        let mut raw = wire::frame_header(32, 1024, &[(rect, Encoding::Raw, px.len())]);
        raw.extend_from_slice(&px);

        let encoded = encode_lossy(&raw, &[rect]);
        let frame = wire::parse_frame(&encoded).unwrap();
        assert_eq!(frame.rects.len(), usize::from(BANDS));
        for part in &frame.rects {
            assert_eq!(part.encoding, Encoding::Jpeg);
            let mut decoder = jpeg_decoder::Decoder::new(part.data);
            let rgb = decoder.decode().unwrap();
            let info = decoder.info().unwrap();
            assert_eq!((info.width, info.height), (part.rect.w, part.rect.h));
            let middle = usize::from(part.rect.h / 2);
            let red = rgb[middle * usize::from(part.rect.w) * 3];
            let expected = ((part.rect.y + part.rect.h / 2) / 8) as u8;
            assert!(
                red.abs_diff(expected) < 16,
                "band at y={}: red {red}, expected {expected}",
                part.rect.y
            );
        }
    }
}
