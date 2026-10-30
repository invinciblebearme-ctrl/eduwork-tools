use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_DIMENSION: u32 = 30_000;
const MAX_PIXELS: u64 = 40_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Jpeg,
    Png,
    Webp,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageOptions {
    pub format: OutputFormat,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub exact: bool,
    pub square_canvas: bool,
    pub quality: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResult {
    pub source: String,
    pub output: Option<String>,
    pub before_bytes: Option<u64>,
    pub after_bytes: Option<u64>,
    pub error: Option<String>,
}

fn validate(options: &ImageOptions) -> Result<(), String> {
    if !(1..=100).contains(&options.quality) {
        return Err("품질은 1~100이어야 합니다.".into());
    }
    if options.width == Some(0) || options.height == Some(0) {
        return Err("크기는 1 이상이어야 합니다.".into());
    }
    if options.width.is_some_and(|v| v > MAX_DIMENSION)
        || options.height.is_some_and(|v| v > MAX_DIMENSION)
    {
        return Err("이미지 크기가 허용 한도(30,000px)를 초과했습니다.".into());
    }
    if options.width.is_some_and(|w| {
        options
            .height
            .is_some_and(|h| u64::from(w) * u64::from(h) > MAX_PIXELS)
    }) {
        return Err("이미지 픽셀 수가 허용 한도(40MP)를 초과했습니다.".into());
    }
    Ok(())
}

fn dimensions(w: u32, h: u32, o: &ImageOptions) -> Result<(u32, u32), String> {
    let (nw, nh) = match (o.width, o.height) {
        (None, None) => (w, h),
        (Some(a), Some(b)) if o.exact => (a, b),
        (Some(a), Some(b)) => {
            let scale = (a as f64 / w as f64).min(b as f64 / h as f64);
            (
                (w as f64 * scale).round().max(1.0) as u32,
                (h as f64 * scale).round().max(1.0) as u32,
            )
        }
        (Some(a), None) => (
            a,
            ((h as f64 * a as f64 / w as f64).round().max(1.0)) as u32,
        ),
        (None, Some(b)) => (
            ((w as f64 * b as f64 / h as f64).round().max(1.0)) as u32,
            b,
        ),
    };
    if nw > MAX_DIMENSION || nh > MAX_DIMENSION || u64::from(nw) * u64::from(nh) > MAX_PIXELS {
        return Err("변환 결과가 허용 크기를 초과했습니다.".into());
    }
    Ok((nw, nh))
}

fn transform(img: DynamicImage, o: &ImageOptions) -> Result<RgbaImage, String> {
    let (w, h) = img.dimensions();
    if w == 0
        || h == 0
        || w > MAX_DIMENSION
        || h > MAX_DIMENSION
        || u64::from(w) * u64::from(h) > MAX_PIXELS
    {
        return Err("원본 이미지 크기가 허용 한도를 초과했습니다.".into());
    }
    let (nw, nh) = dimensions(w, h, o)?;
    let resized = img
        .resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    if o.square_canvas {
        let side = nw.max(nh);
        if u64::from(side) * u64::from(side) > MAX_PIXELS {
            return Err("정사각형 캔버스가 허용 한도(40MP)를 초과했습니다.".into());
        }
        let mut canvas = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
        image::imageops::overlay(
            &mut canvas,
            &resized,
            i64::from((side - nw) / 2),
            i64::from((side - nh) / 2),
        );
        Ok(canvas)
    } else {
        Ok(resized)
    }
}

fn encode(img: &RgbaImage, o: &ImageOptions) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    match o.format {
        OutputFormat::Png => DynamicImage::ImageRgba8(img.clone())
            .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
            .map_err(|e| e.to_string())?,
        OutputFormat::Jpeg => {
            let rgb = image::ImageBuffer::<Rgb<u8>, Vec<u8>>::from_fn(
                img.width(),
                img.height(),
                |x, y| {
                    let pixel = img.get_pixel(x, y).0;
                    let alpha = u16::from(pixel[3]);
                    Rgb([
                        ((u16::from(pixel[0]) * alpha + 255 * (255 - alpha)) / 255) as u8,
                        ((u16::from(pixel[1]) * alpha + 255 * (255 - alpha)) / 255) as u8,
                        ((u16::from(pixel[2]) * alpha + 255 * (255 - alpha)) / 255) as u8,
                    ])
                },
            );
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, o.quality)
                .encode(
                    &rgb,
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| e.to_string())?;
        }
        OutputFormat::Webp => {
            let encoder = webp::Encoder::from_rgba(img.as_raw(), img.width(), img.height());
            bytes = encoder.encode(o.quality as f32).to_vec();
        }
    }
    Ok(bytes)
}

fn output_path(source: &Path, folder: &Path, format: OutputFormat) -> Result<PathBuf, String> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("파일 이름을 읽을 수 없습니다.")?;
    let ext = match format {
        OutputFormat::Jpeg => "jpg",
        OutputFormat::Png => "png",
        OutputFormat::Webp => "webp",
    };
    let mut path = folder.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while fs::symlink_metadata(&path).is_ok() {
        path = folder.join(format!("{stem}_{n}.{ext}"));
        n += 1;
        if n > 10000 {
            return Err("출력 파일 이름을 만들 수 없습니다.".into());
        }
    }
    Ok(path)
}

fn process_one(
    source: &Path,
    folder: &Path,
    options: &ImageOptions,
) -> Result<(PathBuf, u64, u64), String> {
    validate(options)?;
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["jpg", "jpeg", "png", "webp"].contains(&ext.as_str()) {
        return Err("지원하지 않는 파일 형식입니다 (JPG, PNG, WebP만 지원).".into());
    }
    let before = fs::metadata(source).map_err(|e| e.to_string())?.len();
    let reader = image::ImageReader::open(source)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader.format().ok_or("이미지 형식을 확인할 수 없습니다.")?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err("지원하지 않는 래스터 형식입니다.".into());
    }
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("원본 이미지가 허용 한도(40MP)를 초과했습니다.".into());
    }
    let img = image::ImageReader::open(source)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| format!("이미지를 읽을 수 없습니다: {e}"))?;
    let transformed = transform(img, options)?;
    let bytes = encode(&transformed, options)?;
    for _ in 0..10000 {
        let target = output_path(source, folder, options.format)?;
        match crate::output::write_new(&target, &bytes) {
            Ok(()) => return Ok((target, before, bytes.len() as u64)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("출력 파일 이름을 예약할 수 없습니다.".into())
}

pub fn convert_batch(
    paths: Vec<String>,
    output_dir: String,
    options: ImageOptions,
) -> Result<Vec<ImageResult>, String> {
    validate(&options)?;
    let folder = PathBuf::from(output_dir);
    if !folder.is_dir() {
        return Err("출력 폴더를 찾을 수 없습니다.".into());
    }
    Ok(paths
        .into_iter()
        .map(|p| {
            let source = PathBuf::from(&p);
            match process_one(&source, &folder, &options) {
                Ok((out, before, after)) => ImageResult {
                    source: p,
                    output: Some(out.to_string_lossy().into_owned()),
                    before_bytes: Some(before),
                    after_bytes: Some(after),
                    error: None,
                },
                Err(error) => ImageResult {
                    source: p,
                    output: None,
                    before_bytes: fs::metadata(&source).ok().map(|m| m.len()),
                    after_bytes: None,
                    error: Some(error),
                },
            }
        })
        .collect())
}

#[tauri::command]
pub fn convert_images(
    paths: Vec<String>,
    output_dir: String,
    options: ImageOptions,
) -> Result<Vec<ImageResult>, String> {
    convert_batch(paths, output_dir, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "eduwork-image-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn options(format: OutputFormat) -> ImageOptions {
        ImageOptions {
            format,
            width: None,
            height: None,
            exact: false,
            square_canvas: false,
            quality: 80,
        }
    }
    fn fixture(path: &Path, w: u32, h: u32) {
        let img = RgbaImage::from_fn(w, h, |x, y| {
            Rgba([
                (x % 256) as u8,
                (y % 256) as u8,
                120,
                if x == 0 { 0 } else { 255 },
            ])
        });
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("jpg" | "jpeg")
        ) {
            DynamicImage::ImageRgba8(img).to_rgb8().save(path).unwrap()
        } else {
            img.save(path).unwrap()
        }
    }
    #[test]
    fn resize_keeps_aspect_and_exact_mode_works() {
        let mut o = options(OutputFormat::Png);
        o.width = Some(80);
        o.height = Some(80);
        let dims = dimensions(160, 80, &o).unwrap();
        assert_eq!(dims, (80, 40));
        o.exact = true;
        assert_eq!(dimensions(160, 80, &o).unwrap(), (80, 80));
        let t = Temp::new();
        let src = t.0.join("resize.png");
        fixture(&src, 160, 80);
        o.exact = false;
        process_one(&src, &t.0, &o).unwrap();
        assert_eq!(
            image::image_dimensions(t.0.join("resize_1.png")).unwrap(),
            (80, 40)
        );
        o.exact = true;
        process_one(&src, &t.0, &o).unwrap();
        assert_eq!(
            image::image_dimensions(t.0.join("resize_2.png")).unwrap(),
            (80, 80)
        );
    }
    #[test]
    fn rejects_invalid_dimensions_and_oversized_square_canvas() {
        let mut o = options(OutputFormat::Png);
        o.width = Some(0);
        assert!(validate(&o).is_err());
        o.width = Some(30_000);
        o.height = Some(1);
        o.square_canvas = true;
        assert!(transform(
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(30_000, 1, Rgba([1, 2, 3, 255]))),
            &o
        )
        .is_err());
        o.square_canvas = false;
        o.width = Some(8_000);
        o.height = Some(5_001);
        assert!(validate(&o).is_err());
        o.height = Some(5_000);
        assert!(validate(&o).is_ok());
    }
    #[test]
    fn square_canvas_centers_transparent_padding() {
        let mut o = options(OutputFormat::Png);
        o.square_canvas = true;
        let out = transform(
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 2, Rgba([8, 9, 10, 255]))),
            &o,
        )
        .unwrap();
        assert_eq!(out.dimensions(), (4, 4));
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(out.get_pixel(0, 1).0, [8, 9, 10, 255]);
        let t = Temp::new();
        let src = t.0.join("wide.png");
        RgbaImage::from_pixel(4, 2, Rgba([8, 9, 10, 255]))
            .save(&src)
            .unwrap();
        process_one(&src, &t.0, &o).unwrap();
        let saved = image::open(t.0.join("wide_1.png")).unwrap().to_rgba8();
        assert_eq!(saved.dimensions(), (4, 4));
        assert_eq!(saved.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(saved.get_pixel(0, 1).0, [8, 9, 10, 255]);
    }
    #[test]
    fn square_padding_stays_transparent_in_png_webp_and_white_in_jpeg() {
        let mut o = options(OutputFormat::Png);
        o.square_canvas = true;
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 2, Rgba([8, 9, 10, 255])));
        let padded = transform(image, &o).unwrap();

        for format in [OutputFormat::Png, OutputFormat::Webp] {
            o.format = format;
            let bytes = encode(&padded, &o).unwrap();
            let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
            assert_eq!(decoded.get_pixel(0, 0)[3], 0, "{format:?} padding alpha");
        }

        o.format = OutputFormat::Jpeg;
        let bytes = encode(&padded, &o).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
        let padding = decoded.get_pixel(0, 0);
        assert!(
            padding.0.iter().all(|channel| *channel >= 245),
            "JPEG padding should be white, got {padding:?}"
        );
    }
    #[test]
    fn converts_jpeg_png_webp_and_quality_changes_lossy_bytes() {
        let t = Temp::new();
        let src = t.0.join("in.png");
        fixture(&src, 64, 48);
        for f in [OutputFormat::Jpeg, OutputFormat::Png, OutputFormat::Webp] {
            let mut o = options(f);
            let (p, _, _) = process_one(&src, &t.0, &o).unwrap();
            assert!(p.exists());
            if matches!(f, OutputFormat::Webp) {
                o.quality = 20;
                let b = encode(&transform(image::open(&src).unwrap(), &o).unwrap(), &o).unwrap();
                o.quality = 95;
                let a = encode(&transform(image::open(&src).unwrap(), &o).unwrap(), &o).unwrap();
                assert_ne!(a, b);
            } else if matches!(f, OutputFormat::Jpeg) {
                o.quality = 20;
                let low = encode(&transform(image::open(&src).unwrap(), &o).unwrap(), &o).unwrap();
                o.quality = 95;
                let high = encode(&transform(image::open(&src).unwrap(), &o).unwrap(), &o).unwrap();
                assert_ne!(high, low);
            }
        }
    }
    #[test]
    fn output_collision_preserves_original_and_creates_suffixed_file() {
        let t = Temp::new();
        let input = t.0.join("input");
        let output = t.0.join("output");
        fs::create_dir_all(&input).unwrap();
        fs::create_dir_all(&output).unwrap();
        let src = input.join("photo.jpg");
        fixture(&src, 8, 8);
        let original = fs::read(&src).unwrap();
        let expected = output.join("photo.jpg");
        fs::write(&expected, b"existing").unwrap();
        let result = process_one(&src, &output, &options(OutputFormat::Jpeg)).unwrap();
        assert_eq!(result.0.file_name().unwrap(), "photo_1.jpg");
        assert_eq!(fs::read(&src).unwrap(), original);
        assert_eq!(fs::read(expected).unwrap(), b"existing");
    }
    #[test]
    fn batch_continues_after_unsupported_file() {
        let t = Temp::new();
        let valid = t.0.join("good.png");
        fixture(&valid, 4, 4);
        let bad = t.0.join("bad.gif");
        fs::write(&bad, b"gif").unwrap();
        let results = convert_batch(
            vec![
                bad.to_string_lossy().into_owned(),
                valid.to_string_lossy().into_owned(),
            ],
            t.0.to_string_lossy().into_owned(),
            options(OutputFormat::Png),
        )
        .unwrap();
        assert!(results[0].error.is_some());
        assert!(results[1].error.is_none());
    }
}
