//! Zone shapes: opacity of OpenMap's TGA masks (uncompressed or RLE true color with alpha).

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use scout_core::map::Mask;

/// Every `<UiMap id>.tga` mask of a folder (OpenMap's `MapTerritory`).
pub fn zone_masks(dir: &Path) -> Result<HashMap<i64, Mask>> {
    let mut masks = HashMap::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("lecture de {}", dir.display()))? {
        let path = entry?.path();
        let id = path
            .extension()
            .filter(|e| e.eq_ignore_ascii_case("tga"))
            .and_then(|_| path.file_stem()?.to_str()?.parse::<i64>().ok());
        if let Some(id) = id {
            let bytes = std::fs::read(&path)?;
            let mask = alpha_mask(&bytes).with_context(|| format!("{}", path.display()))?;
            masks.insert(id, mask);
        }
    }
    Ok(masks)
}

/// Reads the alpha channel, rows from the top.
pub fn alpha_mask(bytes: &[u8]) -> Result<Mask> {
    ensure!(bytes.len() >= 18, "image TGA tronquée");
    let id_len = bytes[0] as usize;
    let kind = bytes[2];
    let width = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
    let height = u16::from_le_bytes([bytes[14], bytes[15]]) as usize;
    let depth = bytes[16];
    let from_top = bytes[17] & 0x20 != 0;
    if depth != 32 || !matches!(kind, 2 | 10) {
        bail!("image TGA non prise en charge (type {kind}, {depth} bits)");
    }
    let pixels = width * height;
    let mut data = &bytes[(18 + id_len).min(bytes.len())..];
    let mut alpha = Vec::with_capacity(pixels);
    while alpha.len() < pixels {
        if kind == 2 {
            ensure!(data.len() >= 4, "image TGA tronquée");
            alpha.push(data[3]);
            data = &data[4..];
            continue;
        }
        // RLE packet: header, then one pixel repeated or `count` raw pixels.
        ensure!(!data.is_empty(), "image TGA tronquée");
        let (header, rest) = (data[0], &data[1..]);
        let count = (header & 0x7f) as usize + 1;
        if header & 0x80 != 0 {
            ensure!(rest.len() >= 4, "image TGA tronquée");
            alpha.extend(std::iter::repeat_n(rest[3], count));
            data = &rest[4..];
        } else {
            ensure!(rest.len() >= 4 * count, "image TGA tronquée");
            alpha.extend(rest.as_chunks::<4>().0.iter().take(count).map(|p| p[3]));
            data = &rest[4 * count..];
        }
    }
    alpha.truncate(pixels);
    if !from_top {
        alpha = alpha.chunks_exact(width).rev().flatten().copied().collect();
    }
    Ok(Mask {
        width,
        height,
        alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: u8, descriptor: u8) -> Vec<u8> {
        vec![
            0, 0, kind, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 32, descriptor,
        ]
    }

    #[test]
    fn reads_bottom_up_pixels() {
        let mut bytes = header(2, 0x08);
        // Bottom row first: alphas 1, 2 then top row 3, 4.
        for a in [1, 2, 3, 4] {
            bytes.extend([255, 255, 255, a]);
        }
        assert_eq!(alpha_mask(&bytes).unwrap().alpha, [3, 4, 1, 2]);
    }

    #[test]
    fn reads_run_length_packets() {
        let mut bytes = header(10, 0x28);
        bytes.extend([0x82, 0, 0, 0, 9]); // 3 × alpha 9
        bytes.extend([0x00, 0, 0, 0, 7]); // 1 raw pixel, alpha 7
        assert_eq!(alpha_mask(&bytes).unwrap().alpha, [9, 9, 9, 7]);
    }
}
