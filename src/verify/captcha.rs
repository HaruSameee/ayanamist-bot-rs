use crate::Error;
use ab_glyph::{FontArc, PxScale};
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut};
use imageproc::geometric_transformations::{Interpolation, rotate_about_center};
use rand::Rng;
use std::sync::LazyLock;

/// 紛らわしい文字（0 O o 1 l I）を除いた英数字セット。
pub const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const ANSWER_MIN_LEN: usize = 4;
pub const ANSWER_MAX_LEN: usize = 6;

const CELL_WIDTH: u32 = 48;
const IMAGE_HEIGHT: u32 = 96;
const MAX_ROTATION_DEG: f32 = 25.0;

static FONT: LazyLock<Result<FontArc, String>> = LazyLock::new(|| {
    FontArc::try_from_slice(include_bytes!("../../assets/fonts/SpaceMono-Bold.ttf"))
        .map_err(|e| e.to_string())
});

fn font() -> Result<&'static FontArc, Error> {
    match &*FONT {
        Ok(f) => Ok(f),
        Err(e) => Err(format!("フォントの読み込みに失敗: {e}").into()),
    }
}

/// 4〜6文字のランダムな答えを生成する。
pub fn generate_answer<R: Rng>(rng: &mut R) -> String {
    let len = rng.gen_range(ANSWER_MIN_LEN..=ANSWER_MAX_LEN);
    (0..len)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// 答えの文字列から CAPTCHA 画像を生成する。乱数はすべて `rng` 由来で決定的。
pub fn render_captcha<R: Rng>(rng: &mut R, answer: &str) -> Result<DynamicImage, Error> {
    let font = font()?;
    let chars: Vec<char> = answer.chars().collect();
    let width = CELL_WIDTH * chars.len() as u32 + 32;
    let mut canvas = RgbImage::from_pixel(width, IMAGE_HEIGHT, Rgb([255, 255, 255]));

    // 文字ごとに回転・サイズ・ベースラインを変えて合成
    let mut x = 16i64;
    for &c in &chars {
        let scale = rng.gen_range(30.0f32..42.0f32);
        let angle = rng
            .gen_range(-MAX_ROTATION_DEG..=MAX_ROTATION_DEG)
            .to_radians();
        let baseline_jitter = rng.gen_range(-8i32..=8i32);
        let color = Rgb([
            rng.gen_range(0..100u8),
            rng.gen_range(0..100u8),
            rng.gen_range(0..100u8),
        ]);

        let mut glyph_img = RgbImage::from_pixel(64, 64, Rgb([255, 255, 255]));
        draw_text_mut(
            &mut glyph_img,
            color,
            8,
            8,
            PxScale::from(scale),
            font,
            &c.to_string(),
        );
        let rotated = rotate_about_center(&glyph_img, angle, Interpolation::Bilinear, color_bg());
        image::imageops::overlay(&mut canvas, &rotated, x, (16 + baseline_jitter) as i64);
        x += CELL_WIDTH as i64;
    }

    // 文字を横切る直線を2〜4本
    let line_count = rng.gen_range(2..=4);
    for _ in 0..line_count {
        let color = Rgb([
            rng.gen_range(0..200u8),
            rng.gen_range(0..200u8),
            rng.gen_range(0..200u8),
        ]);
        draw_line_segment_mut(
            &mut canvas,
            (
                rng.gen_range(0.0..width as f32),
                rng.gen_range(0.0..IMAGE_HEIGHT as f32),
            ),
            (
                rng.gen_range(0.0..width as f32),
                rng.gen_range(0.0..IMAGE_HEIGHT as f32),
            ),
            color,
        );
    }

    // ノイズ点
    let noise_count = (width * IMAGE_HEIGHT) / 60;
    for _ in 0..noise_count {
        let px = rng.gen_range(0..width);
        let py = rng.gen_range(0..IMAGE_HEIGHT);
        canvas.put_pixel(
            px,
            py,
            Rgb([
                rng.gen_range(0..=255u8),
                rng.gen_range(0..=255u8),
                rng.gen_range(0..=255u8),
            ]),
        );
    }

    // 画像全体に正弦波の歪み
    let amplitude = rng.gen_range(4.0f32..8.0f32);
    let wavelength = rng.gen_range(40.0f32..80.0f32);
    let mut warped = RgbImage::from_pixel(width, IMAGE_HEIGHT, Rgb([255, 255, 255]));
    for y in 0..IMAGE_HEIGHT {
        for x in 0..width {
            let src_y = (y as f32
                + amplitude * (std::f32::consts::TAU * x as f32 / wavelength).sin())
            .round() as i64;
            if (0..IMAGE_HEIGHT as i64).contains(&src_y) {
                let p = canvas.get_pixel(x, src_y as u32);
                warped.put_pixel(x, y, *p);
            }
        }
    }

    Ok(DynamicImage::ImageRgb8(warped))
}

fn color_bg() -> Rgb<u8> {
    Rgb([255, 255, 255])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn generated_answer_has_valid_length_and_charset() {
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..200 {
            let answer = generate_answer(&mut rng);
            assert!((ANSWER_MIN_LEN..=ANSWER_MAX_LEN).contains(&answer.len()));
            assert!(
                answer.bytes().all(|b| CHARSET.contains(&b)),
                "unexpected char in {answer}"
            );
        }
    }

    #[test]
    fn render_is_deterministic_for_same_seed() {
        let answer = "AB23";
        let first = render_captcha(&mut StdRng::seed_from_u64(123), answer)
            .unwrap()
            .to_rgb8();
        let second = render_captcha(&mut StdRng::seed_from_u64(123), answer)
            .unwrap()
            .to_rgb8();
        assert_eq!(first.as_raw(), second.as_raw());
    }

    #[test]
    fn render_differs_for_different_seeds() {
        let answer = "AB23";
        let first = render_captcha(&mut StdRng::seed_from_u64(1), answer)
            .unwrap()
            .to_rgb8();
        let second = render_captcha(&mut StdRng::seed_from_u64(2), answer)
            .unwrap()
            .to_rgb8();
        assert_ne!(first.as_raw(), second.as_raw());
    }

    #[test]
    fn render_supports_all_answer_lengths() {
        for (seed, answer) in ["AB23", "AB234", "AB2345"].into_iter().enumerate() {
            let img = render_captcha(&mut StdRng::seed_from_u64(seed as u64), answer).unwrap();
            assert_eq!(img.height(), IMAGE_HEIGHT);
            assert_eq!(img.width(), CELL_WIDTH * answer.chars().count() as u32 + 32);
        }
    }
}
