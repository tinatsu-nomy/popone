//! sRGB ↔ linear 色空間変換（LUT 実装）
//!
//! `viewer` feature に依存しない純粋な色空間計算ヘルパー。
//! CLI ビルド（`viewer` 無効時）でも `vrm::extract` のミップ生成等から利用可能。

use std::sync::OnceLock;

/// sRGB u8 → linear f32 の LUT（256 エントリ）
static SRGB_TO_LINEAR_LUT: OnceLock<[f32; 256]> = OnceLock::new();

fn srgb_lut() -> &'static [f32; 256] {
    SRGB_TO_LINEAR_LUT.get_or_init(|| {
        let mut lut = [0.0f32; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            let s = i as f32 / 255.0;
            *slot = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            };
        }
        lut
    })
}

/// sRGB → linear (per channel, u8 → f32) LUT バージョン
#[inline(always)]
pub fn srgb_to_linear(c: u8) -> f32 {
    srgb_lut()[c as usize]
}

/// linear f32 → sRGB u8 の LUT（4096 エントリ、量子化リニア値）
static LINEAR_TO_SRGB_LUT: OnceLock<[u8; 4096]> = OnceLock::new();

fn linear_to_srgb_lut() -> &'static [u8; 4096] {
    LINEAR_TO_SRGB_LUT.get_or_init(|| {
        let mut lut = [0u8; 4096];
        for (i, slot) in lut.iter_mut().enumerate() {
            let c = i as f32 / 4095.0;
            let s = if c <= 0.0031308 {
                c * 12.92
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            };
            *slot = (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        lut
    })
}

/// linear → sRGB (per channel, f32 → u8) LUT バージョン
#[inline(always)]
pub fn linear_to_srgb(c: f32) -> u8 {
    let idx = (c.clamp(0.0, 1.0) * 4095.0 + 0.5) as usize;
    linear_to_srgb_lut()[idx.min(4095)]
}

/// sRGB u8 RGBA → linear f32 RGBA (アルファはそのまま 0..1 にマップ)
pub fn rgba8_to_linear_f32(src: &image::RgbaImage) -> image::Rgba32FImage {
    let (w, h) = (src.width(), src.height());
    let mut buf: Vec<f32> = Vec::with_capacity((w * h * 4) as usize);
    for p in src.pixels() {
        buf.push(srgb_to_linear(p[0]));
        buf.push(srgb_to_linear(p[1]));
        buf.push(srgb_to_linear(p[2]));
        buf.push(p[3] as f32 / 255.0); // アルファは線形のまま
    }
    image::Rgba32FImage::from_raw(w, h, buf).expect("Rgba32FImage 構築失敗")
}

/// linear f32 RGBA → sRGB u8 RGBA
pub fn linear_f32_to_rgba8(src: &image::Rgba32FImage) -> image::RgbaImage {
    let (w, h) = (src.width(), src.height());
    let mut buf: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    for p in src.pixels() {
        buf.push(linear_to_srgb(p[0]));
        buf.push(linear_to_srgb(p[1]));
        buf.push(linear_to_srgb(p[2]));
        buf.push((p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    }
    image::RgbaImage::from_raw(w, h, buf).expect("RgbaImage 構築失敗")
}
