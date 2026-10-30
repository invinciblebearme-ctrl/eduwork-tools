#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::{fs, io::Cursor};
    use tempfile::tempdir;

    fn sample_pdf(path: &std::path::Path, label: &str, pages: usize) {
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(
            lopdf::dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"},
        );
        let mut kids = Vec::new();
        for i in 0..pages {
            let page_id = doc.new_object_id();
            let content_id = doc.add_object(lopdf::Stream::new(
                lopdf::dictionary! {},
                format!("BT /F1 12 Tf 20 100 Td ({label}-{i}) Tj ET").into_bytes(),
            ));
            doc.objects.insert(page_id, lopdf::Object::Dictionary(lopdf::dictionary! {"Type"=>"Page", "Parent"=>pages_id, "MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()], "Resources"=>lopdf::dictionary! {"Font"=>lopdf::dictionary! {"F1"=>font_id}}, "Contents"=>content_id}));
            kids.push(page_id.into());
        }
        doc.objects.insert(
            pages_id,
            lopdf::Object::Dictionary(
                lopdf::dictionary! {"Type"=>"Pages", "Kids"=>kids, "Count"=>pages as i64},
            ),
        );
        let catalog_id = doc.add_object(lopdf::dictionary! {"Type"=>"Catalog", "Pages"=>pages_id});
        doc.trailer.set("Root", catalog_id);
        doc.compress();
        doc.save(path).unwrap();
    }
    fn page_count(path: &std::path::Path) -> usize {
        lopdf::Document::load(path).unwrap().get_pages().len()
    }

    #[test]
    fn merges_in_selected_order_and_preserves_sources() {
        let d = tempdir().unwrap();
        let a = d.path().join("a.pdf");
        let b = d.path().join("b.pdf");
        let out = d.path().join("out.pdf");
        sample_pdf(&a, "A", 1);
        sample_pdf(&b, "B", 2);
        merge_pdfs(&[b.clone(), a.clone()], &out).unwrap();
        let merged = lopdf::Document::load(&out).unwrap();
        assert_eq!(merged.get_pages().len(), 3);
        assert!(merged.extract_text(&[1]).unwrap().contains("B-0"));
        assert!(merged.extract_text(&[3]).unwrap().contains("A-0"));
        assert!(a.exists() && b.exists());
    }
    #[test]
    fn extracts_ranges_and_splits_each_page() {
        let d = tempdir().unwrap();
        let source = d.path().join("src.pdf");
        sample_pdf(&source, "P", 3);
        let out = d.path().join("extract.pdf");
        extract_pages(&source, "3,1", &out).unwrap();
        let parsed = lopdf::Document::load(&out).unwrap();
        assert_eq!(parsed.get_pages().len(), 2);
        assert!(parsed.extract_text(&[1]).unwrap().contains("P-2"));
        assert!(parsed.extract_text(&[2]).unwrap().contains("P-0"));
        let dir = d.path().join("split");
        let files = split_pages(&source, &dir).unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.iter().all(|p| page_count(p) == 1));
    }
    #[test]
    fn image_pdf_supports_png_webp_and_reports_pages() {
        let d = tempdir().unwrap();
        let mut paths = Vec::new();
        for (name, format) in [
            ("x.png", ImageFormat::Png),
            ("y.webp", ImageFormat::WebP),
            ("z.jpg", ImageFormat::Jpeg),
        ] {
            let p = d.path().join(name);
            let image = RgbImage::from_pixel(20, 10, image::Rgb([255, 0, 0]));
            image
                .write_to(&mut Cursor::new(Vec::new()), format)
                .unwrap();
            let mut bytes = Vec::new();
            image
                .write_to(&mut Cursor::new(&mut bytes), format)
                .unwrap();
            fs::write(&p, bytes).unwrap();
            paths.push(p);
        }
        let out = d.path().join("images.pdf");
        let n = images_to_pdf(
            &paths,
            &out,
            PaperSize {
                width_mm: 210.0,
                height_mm: 297.0,
            },
            10.0,
        )
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(page_count(&out), 3);
        assert!(paths.iter().all(|p| p.exists()));
        assert!(images_to_pdf(
            &paths,
            &out,
            PaperSize {
                width_mm: 210.0,
                height_mm: 297.0
            },
            10.0
        )
        .is_err());
    }
    #[test]
    fn extracts_odd_even_pages_in_document_order() {
        let d = tempdir().unwrap();
        let source = d.path().join("src.pdf");
        sample_pdf(&source, "P", 4);
        let odd = d.path().join("odd.pdf");
        let even = d.path().join("even.pdf");
        extract_parity(&source, true, &odd).unwrap();
        extract_parity(&source, false, &even).unwrap();
        let odd_doc = lopdf::Document::load(&odd).unwrap();
        let even_doc = lopdf::Document::load(&even).unwrap();
        assert_eq!(odd_doc.get_pages().len(), 2);
        assert_eq!(odd_doc.extract_text(&[1, 2]).unwrap(), "P-0\nP-2\n");
        assert_eq!(even_doc.extract_text(&[1, 2]).unwrap(), "P-1\nP-3\n");
    }

    #[test]
    fn rotates_deletes_and_reorders_pages_without_touching_source() {
        let d = tempdir().unwrap();
        let source = d.path().join("src.pdf");
        sample_pdf(&source, "P", 4);
        let rotated = d.path().join("rotated.pdf");
        rotate_pages(&source, "2,4", 90, &rotated).unwrap();
        let doc = lopdf::Document::load(&rotated).unwrap();
        let pages = doc.get_pages();
        assert_eq!(
            doc.get_object(pages[&1])
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Rotate")
                .ok()
                .and_then(|v| v.as_i64().ok())
                .unwrap_or(0),
            0
        );
        assert_eq!(
            doc.get_object(pages[&2])
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Rotate")
                .unwrap()
                .as_i64()
                .unwrap(),
            90
        );
        let deleted = d.path().join("deleted.pdf");
        delete_pages(&source, "2,4", &deleted).unwrap();
        let deleted_doc = lopdf::Document::load(&deleted).unwrap();
        assert_eq!(deleted_doc.get_pages().len(), 2);
        assert!(deleted_doc.extract_text(&[1, 2]).unwrap().contains("P-0"));
        assert!(deleted_doc.extract_text(&[1, 2]).unwrap().contains("P-2"));
        let reordered = d.path().join("reordered.pdf");
        reorder_pages(&source, "4,2,1,3", &reordered).unwrap();
        let reordered_doc = lopdf::Document::load(&reordered).unwrap();
        assert!(reordered_doc.extract_text(&[1]).unwrap().contains("P-3"));
        assert!(reordered_doc.extract_text(&[2]).unwrap().contains("P-1"));
        assert!(source.exists());
    }

    #[test]
    fn rejects_invalid_rotation_and_empty_delete_without_creating_output() {
        let d = tempdir().unwrap();
        let source = d.path().join("src.pdf");
        sample_pdf(&source, "P", 2);
        let output = d.path().join("out.pdf");
        assert!(rotate_pages(&source, "1", 45, &output).is_err());
        assert!(delete_pages(&source, "1-2", &output).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn rejects_bad_ranges_encrypted_or_invalid_files_and_never_overwrites() {
        let d = tempdir().unwrap();
        let source = d.path().join("src.pdf");
        sample_pdf(&source, "P", 2);
        let out = d.path().join("out.pdf");
        assert_eq!(
            extract_pages(&source, "0,3", &out).unwrap_err(),
            "페이지 번호는 1~2 사이에서 선택해 주세요."
        );
        assert!(parse_ranges("abc", 2).unwrap_err().contains("페이지 범위"));
        assert!(!out.exists());
        fs::write(&out, b"keep").unwrap();
        assert!(extract_pages(&source, "1", &out).is_err());
        assert_eq!(fs::read(&out).unwrap(), b"keep");
        let bad = d.path().join("bad.pdf");
        fs::write(&bad, b"not pdf").unwrap();
        assert!(merge_pdfs(&[bad], &d.path().join("badout.pdf")).is_err());
        let encrypted = d.path().join("encrypted.pdf");
        let mut doc = lopdf::Document::load(&source).unwrap();
        doc.trailer
            .set("Encrypt", lopdf::Object::Dictionary(lopdf::dictionary! {}));
        doc.save(&encrypted).unwrap();
        assert!(extract_pages(&encrypted, "1", &d.path().join("e.pdf"))
            .unwrap_err()
            .contains("암호화된 PDF"));
    }
}
use lopdf::{Document, LoadOptions, Object};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 100;
const MAX_FILE_BYTES: u64 = 250 * 1024 * 1024;
// \uC2E0\uB8B0\uD560 \uC218 \uC5C6\uB294 PDF\uC758 \uAC1D\uCCB4/\uAD50\uCC28 \uCC38\uC870 \uC2A4\uD2B8\uB9BC\uC740 \uAC01\uAC01 64 MiB\uAE4C\uC9C0\uB9CC \uC555\uCD95\uC744 \uD480\uC5B4\uC11C \uC77D\uC2B5\uB2C8\uB2E4.
const MAX_DECOMPRESSED_STREAM_BYTES: usize = 64 * 1024 * 1024;
const MAX_PAGES: usize = 2_000;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_TOTAL_IMAGE_PIXELS: u64 = 80_000_000;

#[derive(Clone, Copy, Debug)]
pub struct PaperSize {
    pub width_mm: f64,
    pub height_mm: f64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOperationResult {
    pub output: String,
    pub file_count: usize,
    pub page_count: usize,
}

#[tauri::command]
pub fn pdf_merge(paths: Vec<String>, output: String) -> Result<PdfOperationResult, String> {
    let inputs = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let output = PathBuf::from(output);
    let page_count = merge_pdfs(&inputs, &output)?;
    Ok(PdfOperationResult {
        output: output.display().to_string(),
        file_count: inputs.len(),
        page_count,
    })
}
#[tauri::command]
pub fn pdf_extract(
    path: String,
    ranges: String,
    output: String,
) -> Result<PdfOperationResult, String> {
    let output = PathBuf::from(output);
    let page_count = extract_pages(Path::new(&path), &ranges, &output)?;
    Ok(PdfOperationResult {
        output: output.display().to_string(),
        file_count: 1,
        page_count,
    })
}
#[tauri::command]
pub fn pdf_split(path: String, output_dir: String) -> Result<PdfOperationResult, String> {
    let files = split_pages(Path::new(&path), Path::new(&output_dir))?;
    Ok(PdfOperationResult {
        output: output_dir,
        file_count: files.len(),
        page_count: files.len(),
    })
}
#[tauri::command]
pub fn pdf_images(
    paths: Vec<String>,
    output: String,
    width_mm: f64,
    height_mm: f64,
    margin_mm: f64,
) -> Result<PdfOperationResult, String> {
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let output = PathBuf::from(output);
    let page_count = images_to_pdf(
        &paths,
        &output,
        PaperSize {
            width_mm,
            height_mm,
        },
        margin_mm,
    )?;
    Ok(PdfOperationResult {
        output: output.display().to_string(),
        file_count: paths.len(),
        page_count,
    })
}

#[tauri::command]
pub fn pdf_parity(path: String, odd: bool, output: String) -> Result<PdfOperationResult, String> {
    let output_path = PathBuf::from(output);
    let page_count = extract_parity(Path::new(&path), odd, &output_path)?;
    Ok(PdfOperationResult {
        output: output_path.display().to_string(),
        file_count: 1,
        page_count,
    })
}
#[tauri::command]
pub fn pdf_rotate(
    path: String,
    ranges: String,
    degrees: i64,
    output: String,
) -> Result<PdfOperationResult, String> {
    let output_path = PathBuf::from(output);
    let page_count = rotate_pages(Path::new(&path), &ranges, degrees, &output_path)?;
    Ok(PdfOperationResult {
        output: output_path.display().to_string(),
        file_count: 1,
        page_count,
    })
}
#[tauri::command]
pub fn pdf_delete(
    path: String,
    ranges: String,
    output: String,
) -> Result<PdfOperationResult, String> {
    let output_path = PathBuf::from(output);
    let page_count = delete_pages(Path::new(&path), &ranges, &output_path)?;
    Ok(PdfOperationResult {
        output: output_path.display().to_string(),
        file_count: 1,
        page_count,
    })
}
#[tauri::command]
pub fn pdf_reorder(
    path: String,
    ranges: String,
    output: String,
) -> Result<PdfOperationResult, String> {
    let output_path = PathBuf::from(output);
    let page_count = reorder_pages(Path::new(&path), &ranges, &output_path)?;
    Ok(PdfOperationResult {
        output: output_path.display().to_string(),
        file_count: 1,
        page_count,
    })
}

fn load_pdf(path: &Path) -> Result<Document, String> {
    let size = fs::metadata(path)
        .map_err(|e| format!("PDF 파일을 찾거나 읽지 못했습니다: {e}"))?
        .len();
    if size == 0 || size > MAX_FILE_BYTES {
        return Err("PDF 파일은 비어 있을 수 없으며 크기는 250 MiB 이하여야 합니다.".into());
    }
    let doc = Document::load_with_options(
        path,
        LoadOptions::with_max_decompressed_size(MAX_DECOMPRESSED_STREAM_BYTES),
    )
    .map_err(|e| format!("손상되었거나 지원하지 않는 PDF입니다: {e}"))?;
    if doc.is_encrypted() || doc.trailer.has(b"Encrypt") {
        return Err("암호화된 PDF는 지원하지 않습니다.".into());
    }
    let count = doc.get_pages().len();
    if count == 0 || count > MAX_PAGES {
        return Err("PDF 페이지 수는 1~2,000페이지여야 합니다.".into());
    }
    Ok(doc)
}

fn save_new(doc: &mut Document, output: &Path) -> Result<(), String> {
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| e.to_string())?;
    crate::output::write_new(output, &bytes).map_err(|e| format!("PDF 저장 실패: {e}"))
}

pub fn merge_pdfs(inputs: &[PathBuf], output: &Path) -> Result<usize, String> {
    if inputs.len() < 2 || inputs.len() > MAX_FILES {
        return Err("병합할 PDF를 2~100개 선택해 주세요.".into());
    }
    let output_abs = output.canonicalize().ok();
    let mut max_id = 1;
    let mut pages = BTreeMap::new();
    let mut objects = BTreeMap::new();
    let mut page_root = None;
    let mut catalog = None;
    let mut total = 0;
    for path in inputs {
        if path.canonicalize().ok() == output_abs && output_abs.is_some() {
            return Err("출력 파일은 입력 PDF와 다른 경로여야 합니다.".into());
        }
        let mut doc = load_pdf(path)?;
        total += doc.get_pages().len();
        if total > MAX_PAGES {
            return Err("병합 결과는 최대 2,000페이지까지 지원합니다.".into());
        }
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;
        for (id, obj) in doc
            .get_pages()
            .into_values()
            .map(|id| (id, doc.objects[&id].clone()))
        {
            pages.insert(id, obj);
        }
        for (id, obj) in doc.objects {
            match obj.type_name().unwrap_or(b"") {
                b"Catalog" => {
                    if catalog.is_none() {
                        catalog = Some((id, obj))
                    }
                }
                b"Pages" => {
                    if page_root.is_none() {
                        page_root = Some((id, obj))
                    }
                }
                b"Page" => {}
                b"Outlines" | b"Outline" => {}
                _ => {
                    objects.insert(id, obj);
                }
            }
        }
    }
    let (root_id, mut root_obj) =
        page_root.ok_or("PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?;
    let root = root_obj
        .as_dict_mut()
        .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?;
    root.set("Count", pages.len() as u32);
    root.set(
        "Kids",
        pages
            .keys()
            .map(|id| Object::Reference(*id))
            .collect::<Vec<_>>(),
    );
    objects.insert(root_id, root_obj);
    for (id, obj) in pages {
        let mut dict = obj
            .as_dict()
            .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
            .clone();
        dict.set("Parent", root_id);
        objects.insert(id, Object::Dictionary(dict));
    }
    let (cat_id, cat_obj) =
        catalog.ok_or("PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?;
    let mut cat = cat_obj
        .as_dict()
        .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
        .clone();
    cat.set("Pages", root_id);
    cat.remove(b"Outlines");
    objects.insert(cat_id, Object::Dictionary(cat));
    let mut merged = Document::with_version("1.5");
    merged.objects = objects;
    merged.max_id = max_id;
    merged.trailer.set("Root", cat_id);
    merged.renumber_objects();
    save_new(&mut merged, output)?;
    Ok(total)
}

fn parse_ranges(spec: &str, count: usize) -> Result<Vec<u32>, String> {
    let syntax = "페이지 범위를 올바르게 입력해 주세요 (예: 1-3,5).";
    if spec.len() > 2000 || spec.trim().is_empty() {
        return Err(syntax.into());
    }
    let mut selected = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(syntax.into());
        }
        let (start, end) = if let Some((a, b)) = part.split_once('-') {
            if b.contains('-') {
                return Err(syntax.into());
            }
            (
                a.trim().parse::<usize>().ok(),
                b.trim().parse::<usize>().ok(),
            )
        } else {
            let n = part.parse::<usize>().ok();
            (n, n)
        };
        let (start, end) = (start.ok_or(syntax)?, end.ok_or(syntax)?);
        if start == 0 || end == 0 || end < start || end > count {
            return Err(format!("페이지 번호는 1~{count} 사이에서 선택해 주세요."));
        }
        for n in start..=end {
            if !seen.insert(n) {
                return Err("같은 페이지 번호를 중복해서 선택했습니다.".into());
            }
            selected.push(n as u32);
            if selected.len() > MAX_PAGES {
                return Err("한 번에 최대 2,000페이지까지 추출할 수 있습니다.".into());
            }
        }
    }
    Ok(selected)
}

fn rewrite_page_order(doc: &mut Document, ordered_pages: &[u32]) -> Result<(), String> {
    let original = doc.get_pages();
    let remove = (1..=original.len() as u32)
        .filter(|n| !ordered_pages.contains(n))
        .collect::<Vec<_>>();
    doc.delete_pages(&remove);
    let page_root = doc
        .get_pages()
        .values()
        .next()
        .and_then(|id| {
            doc.get_object(*id)
                .ok()?
                .as_dict()
                .ok()?
                .get(b"Parent")
                .ok()?
                .as_reference()
                .ok()
        })
        .ok_or("PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?;
    let kids = ordered_pages
        .iter()
        .map(|n| Object::Reference(*original.get(n).expect("validated page number")))
        .collect::<Vec<_>>();
    doc.get_object_mut(page_root)
        .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
        .as_dict_mut()
        .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
        .set("Kids", kids);
    Ok(())
}

pub fn extract_parity(input: &Path, odd: bool, output: &Path) -> Result<usize, String> {
    let mut doc = load_pdf(input)?;
    let count = doc.get_pages().len();
    let selected = (1..=count as u32)
        .filter(|n| (n % 2 == 1) == odd)
        .collect::<Vec<_>>();
    rewrite_page_order(&mut doc, &selected)?;
    save_new(&mut doc, output)?;
    Ok(selected.len())
}

pub fn reorder_pages(input: &Path, spec: &str, output: &Path) -> Result<usize, String> {
    let mut doc = load_pdf(input)?;
    let pages = parse_ranges(spec, doc.get_pages().len())?;
    if pages.len() != doc.get_pages().len() {
        return Err("순서는 모든 페이지를 빠짐없이 한 번씩 지정해야 합니다.".into());
    }
    rewrite_page_order(&mut doc, &pages)?;
    save_new(&mut doc, output)?;
    Ok(pages.len())
}

pub fn delete_pages(input: &Path, spec: &str, output: &Path) -> Result<usize, String> {
    let mut doc = load_pdf(input)?;
    let removed = parse_ranges(spec, doc.get_pages().len())?;
    if removed.len() == doc.get_pages().len() {
        return Err("모든 페이지를 삭제할 수 없습니다.".into());
    }
    let removed = removed
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let kept = (1..=doc.get_pages().len() as u32)
        .filter(|n| !removed.contains(n))
        .collect::<Vec<_>>();
    rewrite_page_order(&mut doc, &kept)?;
    save_new(&mut doc, output)?;
    Ok(kept.len())
}

pub fn rotate_pages(
    input: &Path,
    spec: &str,
    degrees: i64,
    output: &Path,
) -> Result<usize, String> {
    if ![90, 180, 270].contains(&degrees) {
        return Err("회전 각도는 90, 180, 270도 중 선택해 주세요.".into());
    }
    let mut doc = load_pdf(input)?;
    let selected = parse_ranges(spec, doc.get_pages().len())?
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    for (number, id) in doc.get_pages() {
        if !selected.contains(&number) {
            continue;
        }
        let page = doc
            .get_object_mut(id)
            .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
            .as_dict_mut()
            .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?;
        let current = page
            .get(b"Rotate")
            .ok()
            .and_then(|v| v.as_i64().ok())
            .unwrap_or(0);
        page.set("Rotate", (current + degrees) % 360);
    }
    let count = doc.get_pages().len();
    save_new(&mut doc, output)?;
    Ok(count)
}

pub fn extract_pages(input: &Path, spec: &str, output: &Path) -> Result<usize, String> {
    let mut doc = load_pdf(input)?;
    let pages = parse_ranges(spec, doc.get_pages().len())?;
    let original = doc.get_pages();
    let keep = pages
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let remove = (1..=original.len() as u32)
        .filter(|p| !keep.contains(p))
        .collect::<Vec<_>>();
    doc.delete_pages(&remove);
    let page_root = doc
        .get_pages()
        .values()
        .next()
        .and_then(|id| {
            doc.get_object(*id)
                .ok()?
                .as_dict()
                .ok()?
                .get(b"Parent")
                .ok()?
                .as_reference()
                .ok()
        })
        .ok_or("PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?;
    let ordered = pages
        .iter()
        .map(|n| Object::Reference(*original.get(n).expect("validated page number")))
        .collect::<Vec<_>>();
    doc.get_object_mut(page_root)
        .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
        .as_dict_mut()
        .map_err(|_| "PDF 내부 구조가 올바르지 않아 작업을 완료할 수 없습니다.")?
        .set("Kids", ordered);
    save_new(&mut doc, output)?;
    Ok(pages.len())
}
pub fn split_pages(input: &Path, output_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let doc = load_pdf(input)?;
    let count = doc.get_pages().len();
    fs::create_dir_all(output_dir)
        .map_err(|e| format!("분할 결과 폴더를 만들지 못했습니다: {e}"))?;
    let outputs = (1..=count)
        .map(|n| output_dir.join(format!("page-{n:03}.pdf")))
        .collect::<Vec<_>>();
    if outputs.iter().any(|p| p.exists()) {
        return Err("같은 이름의 분할 결과가 이미 있습니다. 빈 폴더를 선택해 주세요.".into());
    }
    let mut result = Vec::new();
    for (idx, path) in outputs.into_iter().enumerate() {
        let mut part = doc.clone();
        let remove = (1..=count as u32)
            .filter(|n| *n != (idx + 1) as u32)
            .collect::<Vec<_>>();
        part.delete_pages(&remove);
        save_new(&mut part, &path)?;
        result.push(path);
    }
    Ok(result)
}

pub fn images_to_pdf(
    paths: &[PathBuf],
    output: &Path,
    paper: PaperSize,
    margin_mm: f64,
) -> Result<usize, String> {
    use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, RawImage, XObjectTransform};
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("PDF로 변환할 이미지는 1~100개 선택해 주세요.".into());
    }
    if !paper.width_mm.is_finite()
        || !paper.height_mm.is_finite()
        || !(20.0..=1000.0).contains(&paper.width_mm)
        || !(20.0..=1000.0).contains(&paper.height_mm)
        || !margin_mm.is_finite()
        || margin_mm < 0.0
        || margin_mm * 2.0 >= paper.width_mm.min(paper.height_mm)
    {
        return Err(
            "용지 크기는 각 20~1,000mm, 여백은 0 이상이며 짧은 변의 절반보다 작아야 합니다.".into(),
        );
    }
    let mut doc = PdfDocument::new("EduWork Tools Image PDF");
    let mut pages = Vec::new();
    let mut total_pixels = 0u64;
    for path in paths {
        let bytes = fs::read(path).map_err(|e| format!("이미지 파일을 읽지 못했습니다: {e}"))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("이미지 파일은 250 MiB 이하여야 합니다.".into());
        }
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| format!("이미지 형식을 확인하지 못했습니다: {e}"))?
            .into_dimensions()
            .map_err(|e| format!("이미지 크기를 읽지 못했습니다: {e}"))?;
        let pixels = u64::from(w) * u64::from(h);
        if pixels > MAX_IMAGE_PIXELS {
            return Err("이미지 한 장은 최대 40MP까지 지원합니다.".into());
        }
        total_pixels = total_pixels.saturating_add(pixels);
        if total_pixels > MAX_TOTAL_IMAGE_PIXELS {
            return Err("한 번의 작업에서 이미지 총 픽셀 수는 최대 80MP입니다.".into());
        }
        let mut warnings = Vec::new();
        let raw = RawImage::decode_from_bytes(&bytes, &mut warnings)
            .map_err(|e| format!("지원하지 않거나 손상된 JPG/PNG/WebP 이미지입니다: {e}"))?;
        let id = doc.add_image(&raw);
        let avail_w = paper.width_mm - 2.0 * margin_mm;
        let avail_h = paper.height_mm - 2.0 * margin_mm;
        let scale = (avail_w / (w as f64 * 25.4 / 300.0)).min(avail_h / (h as f64 * 25.4 / 300.0));
        let draw_w = w as f64 * 25.4 / 300.0 * scale;
        let draw_h = h as f64 * 25.4 / 300.0 * scale;
        let transform = XObjectTransform {
            translate_x: Some(Mm(((paper.width_mm - draw_w) / 2.0) as f32).into()),
            translate_y: Some(Mm(((paper.height_mm + draw_h) / 2.0) as f32).into()),
            ..Default::default()
        };
        pages.push(PdfPage::new(
            Mm(paper.width_mm as f32),
            Mm(paper.height_mm as f32),
            vec![Op::UseXobject { id, transform }],
        ));
    }
    doc.with_pages(pages);
    let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
    crate::output::write_new(output, &bytes).map_err(|e| format!("PDF 저장 실패: {e}"))?;
    Ok(paths.len())
}
