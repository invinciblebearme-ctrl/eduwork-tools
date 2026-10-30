use calamine::{open_workbook, Data, Reader, Xlsx};
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 50;
const MAX_BYTES: u64 = 100 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 250 * 1024 * 1024;
const MAX_ROWS: usize = 100_000;
const MAX_CELLS: usize = 1_000_000;
const MAX_SHEETS: usize = 200;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanOptions {
    pub remove_duplicates: bool,
    pub remove_blank_rows: bool,
    pub trim_text: bool,
    pub sort_column: Option<usize>,
    pub filter_column: Option<usize>,
    pub filter_value: Option<String>,
    pub contains: bool,
}

#[derive(Serialize)]
pub struct ExcelResult {
    pub outputs: Vec<String>,
    pub rows: usize,
    pub sheets: usize,
    pub message: String,
}

fn validate_input(path: &Path) -> Result<(), String> {
    let meta = fs::metadata(path).map_err(|e| format!("입력 파일을 읽을 수 없습니다: {e}"))?;
    if !meta.is_file() {
        return Err("입력 경로가 파일이 아닙니다.".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("파일이 100 MiB 제한을 초과합니다.".into());
    }
    Ok(())
}
fn ensure_new(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("출력 파일이 이미 있습니다: {}", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("출력 폴더를 만들 수 없습니다: {e}"))?;
    }
    Ok(())
}
fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        _ => cell.to_string(),
    }
}
pub(crate) fn compare_sort_values(a: Option<&String>, b: Option<&String>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
            (Ok(a_num), Ok(b_num)) if a_num.is_finite() && b_num.is_finite() => {
                a_num.total_cmp(&b_num)
            }
            _ => a.to_lowercase().cmp(&b.to_lowercase()),
        },
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
    }
}
pub(crate) fn preflight_new_paths(paths: &[PathBuf]) -> Result<(), String> {
    let mut planned = HashSet::new();
    for path in paths {
        let key = path.to_string_lossy().to_lowercase();
        if !planned.insert(key) {
            return Err(format!(
                "출력 파일 이름이 서로 겹칩니다: {}",
                path.display()
            ));
        }
        if path.exists() {
            return Err(format!("출력 파일이 이미 있습니다: {}", path.display()));
        }
        if let Some(parent) = path.parent() {
            if parent.exists() && !parent.is_dir() {
                return Err(format!("출력 경로가 폴더가 아닙니다: {}", parent.display()));
            }
        }
    }
    Ok(())
}
fn workbook_data(path: &Path) -> Result<Vec<(String, Vec<Vec<String>>)>, String> {
    validate_input(path)?;
    let mut book: Xlsx<_> = open_workbook(path)
        .map_err(|e| format!("XLSX 파일이 손상되었거나 지원되지 않습니다: {e}"))?;
    let names = book.sheet_names().to_vec();
    if names.is_empty() {
        return Err("워크북에 시트가 없습니다.".into());
    }
    if names.len() > MAX_SHEETS {
        return Err("워크북은 최대 200개 시트까지 처리할 수 있습니다.".into());
    }
    let mut all = Vec::new();
    let mut workbook_cells = 0usize;
    for name in names {
        let range = book
            .worksheet_range(&name)
            .map_err(|e| format!("시트 '{name}'을 읽지 못했습니다: {e}"))?;
        if range.height() > MAX_ROWS || range.height().saturating_mul(range.width()) > MAX_CELLS {
            return Err(format!("시트 '{name}'이 행 또는 셀 제한을 초과합니다."));
        }
        workbook_cells =
            workbook_cells.saturating_add(range.height().saturating_mul(range.width()));
        if workbook_cells > MAX_CELLS {
            return Err("워크북 전체 셀이 1,000,000개 제한을 초과합니다.".into());
        }
        let rows = range
            .rows()
            .map(|row| row.iter().map(cell_text).collect())
            .collect();
        all.push((name, rows));
    }
    Ok(all)
}
fn write_book(path: &Path, sheets: &[(String, Vec<Vec<String>>)]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("출력 폴더를 만들 수 없습니다: {e}"))?;
    }
    if sheets.is_empty() {
        return Err("저장할 시트가 없습니다.".into());
    }
    let mut book = Workbook::new();
    for (name, rows) in sheets {
        if rows.len() > MAX_ROWS || rows.iter().map(Vec::len).sum::<usize>() > MAX_CELLS {
            return Err("출력 행 또는 셀 제한을 초과합니다.".into());
        }
        let sheet = book.add_worksheet();
        let safe_name: String = name
            .chars()
            .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
            .take(31)
            .collect();
        sheet
            .set_name(if safe_name.is_empty() {
                "Sheet"
            } else {
                &safe_name
            })
            .map_err(|e| format!("시트 이름이 올바르지 않습니다: {e}"))?;
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in row.iter().enumerate() {
                sheet
                    .write_string(r as u32, c as u16, value)
                    .map_err(|e| format!("셀을 쓰지 못했습니다: {e}"))?;
            }
        }
    }
    let bytes = book
        .save_to_buffer()
        .map_err(|e| format!("XLSX 생성 실패: {e}"))?;
    crate::output::write_new(path, &bytes).map_err(|e| format!("XLSX 저장 실패: {e}"))?;
    Ok(())
}
pub fn read_sheet(path: &Path, name: &str) -> Result<Vec<Vec<String>>, String> {
    workbook_data(path)?
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| r)
        .ok_or_else(|| format!("'{name}' 시트를 찾을 수 없습니다."))
}
pub fn inspect(path: &Path) -> Result<Vec<(String, Vec<Vec<String>>)>, String> {
    workbook_data(path)
}
pub fn write_single(path: &Path, name: &str, rows: &[Vec<String>]) -> Result<(), String> {
    write_book(path, &[(name.to_owned(), rows.to_vec())])
}

pub fn merge_workbooks(
    paths: &[PathBuf],
    output: &Path,
    keep_header: bool,
    source_column: Option<String>,
) -> Result<ExcelResult, String> {
    if paths.len() < 2 || paths.len() > MAX_FILES {
        return Err("워크북은 2~50개 선택해야 합니다.".into());
    }
    let mut total_bytes = 0u64;
    for path in paths {
        validate_input(path)?;
        total_bytes =
            total_bytes.saturating_add(fs::metadata(path).map_err(|e| e.to_string())?.len());
    }
    if total_bytes > MAX_TOTAL_BYTES {
        return Err("선택한 파일의 전체 크기가 250 MiB 제한을 초과합니다.".into());
    }
    let mut merged: Vec<(String, Vec<Vec<String>>)> = Vec::new();
    for (idx, path) in paths.iter().enumerate() {
        let data = workbook_data(path)?;
        if idx == 0 {
            merged = data;
            if let Some(col) = &source_column {
                for (_, rows) in &mut merged {
                    if !rows.is_empty() {
                        rows[0].push(col.clone());
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        for row in rows.iter_mut().skip(1) {
                            row.push(filename.to_string());
                        }
                    }
                }
            }
            continue;
        }
        if data.len() != merged.len() {
            return Err("시트 구성이 서로 다릅니다.".into());
        }
        for (name, rows) in data {
            let (_, target) = merged
                .iter_mut()
                .find(|(n, _)| n == &name)
                .ok_or("시트 이름 구성이 서로 다릅니다.")?;
            let extra_source_column = usize::from(source_column.is_some());
            let header_matches = match (target.first(), rows.first()) {
                (Some(target_header), Some(input_header)) => {
                    target_header.len() == input_header.len() + extra_source_column
                        && target_header
                            .iter()
                            .take(input_header.len())
                            .eq(input_header.iter())
                }
                (None, None) => true,
                _ => false,
            };
            if target.is_empty() != rows.is_empty()
                || target
                    .first()
                    .map(Vec::len)
                    .map(|n| n.saturating_sub(extra_source_column))
                    != rows.first().map(Vec::len)
                || (keep_header && !header_matches)
            {
                return Err(format!("'{name}' 시트의 머리글/열 구조가 서로 다릅니다."));
            }
            let start = usize::from(keep_header && !rows.is_empty());
            for mut row in rows.into_iter().skip(start) {
                if source_column.is_some() {
                    row.push(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
                target.push(row);
            }
        }
    }
    let rows = merged.iter().map(|(_, r)| r.len()).sum();
    write_book(output, &merged)?;
    Ok(ExcelResult {
        outputs: vec![output.display().to_string()],
        rows,
        sheets: merged.len(),
        message: "값만 새 워크북에 저장했습니다.".into(),
    })
}
pub fn split_workbook(
    input: &Path,
    dir: &Path,
    selected: Option<&str>,
    explicit: Option<PathBuf>,
) -> Result<ExcelResult, String> {
    let sheets = workbook_data(input)?;
    let chosen: Vec<_> = sheets
        .into_iter()
        .filter(|(n, _)| selected.map_or(true, |s| s == n))
        .collect();
    if chosen.is_empty() {
        return Err("선택한 시트를 찾을 수 없습니다.".into());
    }
    let mut planned = Vec::new();
    for (name, _) in &chosen {
        let out = if let Some(p) = explicit.as_ref() {
            p.clone()
        } else {
            dir.join(format!(
                "{}.xlsx",
                name.replace(['\\', '/', ';', ':', '*', '?', '"', '<', '>', '|'], "_")
            ))
        };
        planned.push(out);
    }
    if dir.exists() && !dir.is_dir() {
        return Err(format!("출력 경로가 폴더가 아닙니다: {}", dir.display()));
    }
    preflight_new_paths(&planned)?;
    let mut outputs = Vec::new();
    let mut rows = 0;
    for ((name, data), out) in chosen.into_iter().zip(planned) {
        write_book(&out, &[(name, data.clone())])?;
        rows += data.len();
        outputs.push(out.display().to_string());
    }
    let sheet_count = outputs.len();
    Ok(ExcelResult {
        outputs,
        rows,
        sheets: sheet_count,
        message: "시트별 파일을 저장했습니다.".into(),
    })
}
pub fn clean_sheet(
    input: &Path,
    name: &str,
    output: &Path,
    options: CleanOptions,
) -> Result<ExcelResult, String> {
    let mut rows = read_sheet(input, name)?;
    for row in &mut rows {
        if options.trim_text {
            for cell in row {
                *cell = cell.trim().to_owned();
            }
        }
    }
    if options.remove_blank_rows {
        rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    }
    if options.remove_duplicates {
        let mut seen = HashSet::new();
        rows.retain(|r| seen.insert(r.clone()));
    }
    if let Some(col) = options.sort_column {
        if col == 0 {
            return Err("정렬 열은 1부터 입력하세요.".into());
        }
        let header = if rows.is_empty() {
            vec![]
        } else {
            vec![rows.remove(0)]
        };
        rows.sort_by(|a, b| compare_sort_values(a.get(col - 1), b.get(col - 1)));
        rows.splice(0..0, header);
    }
    if let (Some(col), Some(val)) = (options.filter_column, options.filter_value) {
        if col == 0 {
            return Err("필터 열은 1부터 입력하세요.".into());
        }
        let header = if rows.is_empty() {
            vec![]
        } else {
            vec![rows.remove(0)]
        };
        rows.retain(|r| {
            r.get(col - 1).map_or(false, |v| {
                if options.contains {
                    v.contains(&val)
                } else {
                    v == &val
                }
            })
        });
        rows.splice(0..0, header);
    }
    let count = rows.len();
    write_book(output, &[(name.to_owned(), rows)])?;
    Ok(ExcelResult {
        outputs: vec![output.display().to_string()],
        rows: count,
        sheets: 1,
        message: "정리한 새 파일을 저장했습니다.".into(),
    })
}
pub fn xlsx_to_csv(
    input: &Path,
    sheet: &str,
    output: &Path,
    bom: bool,
) -> Result<ExcelResult, String> {
    let rows = read_sheet(input, sheet)?;
    ensure_new(output)?;
    let result: Result<(), String> = (|| {
        let mut bytes = Vec::new();
        let mut w = csv::Writer::from_writer(&mut bytes);
        for row in &rows {
            w.write_record(row).map_err(|e| e.to_string())?
        }
        w.flush().map_err(|e| e.to_string())?;
        drop(w);
        let mut data = if bom {
            vec![0xEF, 0xBB, 0xBF]
        } else {
            Vec::new()
        };
        data.extend(bytes);
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        return Err(format!("CSV 저장 실패: {e}"));
    }
    Ok(ExcelResult {
        outputs: vec![output.display().to_string()],
        rows: rows.len(),
        sheets: 1,
        message: "UTF-8 CSV를 저장했습니다.".into(),
    })
}
pub fn csv_to_xlsx(input: &Path, output: &Path) -> Result<ExcelResult, String> {
    validate_input(input)?;
    let mut bytes = fs::read(input).map_err(|e| e.to_string())?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(bytes.as_slice());
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut cells = 0usize;
    for record in reader.records() {
        let record = record.map_err(|e| format!("CSV 형식 오류: {e}"))?;
        cells = cells.saturating_add(record.len());
        if rows.len() >= MAX_ROWS || cells > MAX_CELLS {
            return Err("CSV가 행 또는 셀 제한을 초과합니다.".into());
        }
        rows.push(record.iter().map(str::to_owned).collect());
    }
    write_book(
        output,
        &[(
            input
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            rows.clone(),
        )],
    )?;
    Ok(ExcelResult {
        outputs: vec![output.display().to_string()],
        rows: rows.len(),
        sheets: 1,
        message: "CSV를 새 XLSX로 저장했습니다.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{open_workbook, Reader, Xlsx};
    use rust_xlsxwriter::Workbook;
    use tempfile::tempdir;

    fn workbook(path: &std::path::Path, sheets: &[(&str, &[&[&str]])]) {
        let mut book = Workbook::new();
        for (name, rows) in sheets {
            let sheet = book.add_worksheet();
            sheet.set_name(*name).unwrap();
            for (r, row) in rows.iter().enumerate() {
                for (c, value) in row.iter().enumerate() {
                    sheet.write_string(r as u32, c as u16, *value).unwrap();
                }
            }
        }
        book.save(path).unwrap();
    }

    #[test]
    fn merge_matches_sheet_names_and_retains_one_header() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("가.xlsx");
        let b = dir.path().join("나.xlsx");
        let out = dir.path().join("merged.xlsx");
        workbook(&a, &[("학생", &[&["이름", "반"], &["민지", "1"]])]);
        workbook(&b, &[("학생", &[&["이름", "반"], &["준호", "2"]])]);
        merge_workbooks(&[a, b], &out, true, None).unwrap();
        let mut x: Xlsx<_> = open_workbook(&out).unwrap();
        let rows = x.worksheet_range("학생").unwrap();
        assert_eq!(rows.height(), 3);
        assert_eq!(rows.get((2, 0)).unwrap().to_string(), "준호");
    }

    #[test]
    fn merge_source_column_keeps_compatible_header_structure() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("가.xlsx");
        let b = dir.path().join("나.xlsx");
        let out = dir.path().join("merged.xlsx");
        workbook(&a, &[("학생", &[&["이름"], &["민지"]])]);
        workbook(&b, &[("학생", &[&["이름"], &["준호"]])]);
        merge_workbooks(&[a, b], &out, true, Some("원본 파일".into())).unwrap();
        let rows = read_sheet(&out, "학생").unwrap();
        assert_eq!(rows[0], vec!["이름", "원본 파일"]);
        assert_eq!(rows[2][0], "준호");
    }

    #[test]
    fn merge_source_filename_is_present_on_every_data_row_from_every_workbook() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("first.xlsx");
        let b = dir.path().join("second.xlsx");
        let out = dir.path().join("merged.xlsx");
        workbook(&a, &[("Roster", &[&["Name"], &["Minji"], &["Ara"]])]);
        workbook(&b, &[("Roster", &[&["Name"], &["Junho"]])]);

        merge_workbooks(&[a, b], &out, true, Some("Source".into())).unwrap();

        let rows = read_sheet(&out, "Roster").unwrap();
        assert_eq!(rows[0], vec!["Name", "Source"]);
        assert_eq!(rows[1][1], "first.xlsx");
        assert_eq!(rows[2][1], "first.xlsx");
        assert_eq!(rows[3][1], "second.xlsx");
    }

    #[test]
    fn clean_sort_orders_numeric_student_numbers_numerically() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("students.xlsx");
        let out = dir.path().join("sorted.xlsx");
        workbook(
            &input,
            &[("Roster", &[&["Student number"], &["10"], &["2"]])],
        );

        clean_sheet(
            &input,
            "Roster",
            &out,
            CleanOptions {
                sort_column: Some(1),
                ..Default::default()
            },
        )
        .unwrap();

        let rows = read_sheet(&out, "Roster").unwrap();
        assert_eq!(rows[1][0], "2");
        assert_eq!(rows[2][0], "10");
    }

    #[test]
    fn merge_rejects_incompatible_headers_and_preserves_existing_destination() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.xlsx");
        let b = dir.path().join("b.xlsx");
        let out = dir.path().join("existing.xlsx");
        workbook(&a, &[("Roster", &[&["Name", "Class"], &["Minji", "1"]])]);
        workbook(&b, &[("Roster", &[&["Name", "Grade"], &["Junho", "2"]])]);
        assert!(merge_workbooks(&[a, b], &out, true, None).is_err());
        fs::write(&out, b"keep this file").unwrap();
        assert!(merge_workbooks(
            &[dir.path().join("a.xlsx"), dir.path().join("a.xlsx")],
            &out,
            true,
            None
        )
        .is_err());
        assert_eq!(fs::read(&out).unwrap(), b"keep this file");
    }

    #[test]
    fn split_extracts_only_requested_sheet_without_overwrite() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("source.xlsx");
        let out = dir.path().join("chosen.xlsx");
        workbook(&input, &[("A", &[&["x"]]), ("B", &[&["y"]])]);
        split_workbook(
            &input,
            &dir.path().join("split"),
            Some("B"),
            Some(out.clone()),
        )
        .unwrap();
        assert_eq!(read_sheet(&out, "B").unwrap()[0][0], "y");
        assert!(split_workbook(&input, &dir.path().join("split"), Some("B"), Some(out)).is_err());
    }

    #[test]
    fn split_preflights_sanitized_filename_collisions_before_writing() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("source.xlsx");
        let output_dir = dir.path().join("split");
        workbook(&input, &[("A;B", &[&["x"]]), ("A_B", &[&["y"]])]);

        assert!(split_workbook(&input, &output_dir, None, None).is_err());
        assert!(!output_dir.exists());
    }

    #[test]
    fn split_preflights_existing_output_before_writing_any_sheet() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("source.xlsx");
        let output_dir = dir.path().join("split");
        fs::create_dir(&output_dir).unwrap();
        workbook(&input, &[("A", &[&["x"]]), ("B", &[&["y"]])]);
        fs::write(output_dir.join("B.xlsx"), b"keep").unwrap();

        assert!(split_workbook(&input, &output_dir, None, None).is_err());
        assert!(!output_dir.join("A.xlsx").exists());
        assert_eq!(fs::read(output_dir.join("B.xlsx")).unwrap(), b"keep");
    }

    #[test]
    fn clean_and_csv_round_trip_preserve_korean_values() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("입력.xlsx");
        let cleaned = dir.path().join("정리.xlsx");
        let csv_path = dir.path().join("자료.csv");
        let back = dir.path().join("다시.xlsx");
        workbook(
            &input,
            &[(
                "명단",
                &[
                    &["이름", "번호"],
                    &[" 민지 ", "2"],
                    &["민지", "2"],
                    &["", ""],
                ],
            )],
        );
        clean_sheet(
            &input,
            "명단",
            &cleaned,
            CleanOptions {
                remove_duplicates: true,
                remove_blank_rows: true,
                trim_text: true,
                ..Default::default()
            },
        )
        .unwrap();
        xlsx_to_csv(&cleaned, "명단", &csv_path, true).unwrap();
        csv_to_xlsx(&csv_path, &back).unwrap();
        let rows = read_sheet(&back, "자료").unwrap();
        assert_eq!(rows[1][0], "민지");
        assert_eq!(rows.len(), 2);
    }
}
