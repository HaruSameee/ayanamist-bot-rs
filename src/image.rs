use crate::Error;
use image::{DynamicImage, Pixel, Rgb, RgbImage};

pub fn encode_webp(img: &DynamicImage) -> Result<Vec<u8>, Error> {
    Ok(webp::Encoder::from_image(img)?.encode(90f32).to_vec())
}

pub fn alpha_to_mask(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let mut mask = RgbImage::new(w, h);

    for (x, y, p) in rgba.enumerate_pixels() {
        let alpha = p[3];
        let v = if alpha == 0 { 0 } else { 255 };
        mask.put_pixel(x, y, Rgb([v, v, v]));
    }

    DynamicImage::ImageRgb8(mask)
}

pub fn background(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let mut mask = RgbImage::new(w, h);

    for (x, y, p) in rgba.enumerate_pixels() {
        let alpha = p[3];
        let rgb = if alpha == 0 {
            Rgb([0, 0, 0])
        } else {
            p.to_rgb()
        };

        mask.put_pixel(x, y, rgb);
    }

    DynamicImage::ImageRgb8(mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn rgba_image(pixels: &[[u8; 4]]) -> DynamicImage {
        let mut img = RgbaImage::new(pixels.len() as u32, 1);
        for (x, p) in pixels.iter().enumerate() {
            img.put_pixel(x as u32, 0, Rgba(*p));
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn alpha_to_mask_makes_transparent_pixel_black() {
        let img = rgba_image(&[[10, 20, 30, 0]]);
        let mask = alpha_to_mask(&img).to_rgb8();

        assert_eq!(mask.get_pixel(0, 0), &Rgb([0, 0, 0]));
    }

    #[test]
    fn alpha_to_mask_makes_nonzero_alpha_white() {
        // 境界値 alpha=1, 254 も白になること
        let img = rgba_image(&[[10, 20, 30, 1], [10, 20, 30, 254], [10, 20, 30, 255]]);
        let mask = alpha_to_mask(&img).to_rgb8();

        for x in 0..3 {
            assert_eq!(mask.get_pixel(x, 0), &Rgb([255, 255, 255]));
        }
    }

    #[test]
    fn background_makes_transparent_pixel_black() {
        let img = rgba_image(&[[10, 20, 30, 0]]);
        let bg = background(&img).to_rgb8();

        assert_eq!(bg.get_pixel(0, 0), &Rgb([0, 0, 0]));
    }

    #[test]
    fn background_keeps_rgb_for_nonzero_alpha() {
        let img = rgba_image(&[[10, 20, 30, 1], [200, 100, 50, 255]]);
        let bg = background(&img).to_rgb8();

        assert_eq!(bg.get_pixel(0, 0), &Rgb([10, 20, 30]));
        assert_eq!(bg.get_pixel(1, 0), &Rgb([200, 100, 50]));
    }

    #[test]
    fn encode_webp_outputs_webp_magic_bytes() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(8, 8));
        let bytes = encode_webp(&img).unwrap();

        assert!(bytes.len() >= 12);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn png_roundtrip_via_guessed_format() {
        // pokemon/command.rs のスプライト画像デコード経路と同じ形で PNG が扱えることを担保する
        use image::ImageReader;
        use std::io::Cursor;

        let img = rgba_image(&[[10, 20, 30, 255], [200, 100, 50, 255]]);
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();

        let decoded = ImageReader::new(Cursor::new(buf.into_inner()))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();

        assert_eq!(decoded.width(), img.width());
        assert_eq!(decoded.height(), img.height());
    }
}
