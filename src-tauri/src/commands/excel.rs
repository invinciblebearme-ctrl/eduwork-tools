use crate::excel::{self, CleanOptions, ExcelResult};
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn excel_inspect(path: String) -> Result<Vec<SheetInfo>, String> {
    let data = excel::inspect(Path::new(&path))?;
    Ok(data
        .into_iter()
        .map(|(name, rows)| SheetInfo {
            name,
            rows: rows.len(),
            columns: rows.first().map_or(0, Vec::len),
            preview: rows.into_iter().take(8).collect(),
        })
        .collect())
}
#[derive(serde::Serialize)]
pub struct SheetInfo {
    pub name: String,
    pub rows: usize,
    pub columns: usize,
    pub preview: Vec<Vec<String>>,
}
#[tauri::command]
pub fn excel_merge(
    paths: Vec<String>,
    output: String,
    keep_header: bool,
    source_column: Option<String>,
) -> Result<ExcelResult, String> {
    excel::merge_workbooks(
        &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
        Path::new(&output),
        keep_header,
        source_column,
    )
}
#[tauri::command]
pub fn excel_split(
    path: String,
    output_dir: String,
    sheet: Option<String>,
    output: Option<String>,
) -> Result<ExcelResult, String> {
    excel::split_workbook(
        Path::new(&path),
        Path::new(&output_dir),
        sheet.as_deref(),
        output.map(PathBuf::from),
    )
}
#[tauri::command]
pub fn excel_clean(
    path: String,
    sheet: String,
    output: String,
    options: CleanOptions,
) -> Result<ExcelResult, String> {
    excel::clean_sheet(Path::new(&path), &sheet, Path::new(&output), options)
}
#[tauri::command]
pub fn excel_to_csv(
    path: String,
    sheet: String,
    output: String,
    bom: bool,
) -> Result<ExcelResult, String> {
    excel::xlsx_to_csv(Path::new(&path), &sheet, Path::new(&output), bom)
}
#[tauri::command]
pub fn csv_to_excel(path: String, output: String) -> Result<ExcelResult, String> {
    excel::csv_to_xlsx(Path::new(&path), Path::new(&output))
}
#[tauri::command]
pub fn excel_roster(
    paths: Vec<String>,
    output: String,
    grade_column: Option<usize>,
    class_column: Option<usize>,
    duplicate_column: Option<usize>,
    sort_column: Option<usize>,
    keep_header: bool,
) -> Result<RosterResult, String> {
    if paths.len() > 50 {
        return Err("워크북은 최대 50개까지 가능합니다.".into());
    }
    if [grade_column, class_column, duplicate_column, sort_column]
        .into_iter()
        .flatten()
        .any(|column| column == 0)
    {
        return Err("열 번호는 1부터 입력하세요.".into());
    }
    let input_paths = paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    let grouping = grade_column.is_some() || class_column.is_some();
    let mut planned_outputs = vec![PathBuf::from(&output)];
    if grouping || sort_column.is_some() {
        let mut sheet_names = Vec::<String>::new();
        let mut group_keys = std::collections::BTreeMap::<
            String,
            std::collections::BTreeSet<(String, String)>,
        >::new();
        for path in &input_paths {
            for (name, rows) in excel::inspect(path)? {
                if !sheet_names.contains(&name) {
                    sheet_names.push(name.clone());
                }
                if grouping {
                    for row in rows.iter().skip(1) {
                        let grade = grade_column
                            .and_then(|col| row.get(col - 1))
                            .cloned()
                            .unwrap_or_default();
                        let class = class_column
                            .and_then(|col| row.get(col - 1))
                            .cloned()
                            .unwrap_or_default();
                        group_keys
                            .entry(name.clone())
                            .or_default()
                            .insert((grade, class));
                    }
                }
            }
        }
        for name in sheet_names {
            if let Some(keys) = group_keys.get(&name) {
                for (grade, class) in keys {
                    planned_outputs.push(roster_group_path(
                        &output,
                        &name,
                        grade,
                        class,
                        grade_column.is_some(),
                        class_column.is_some(),
                    ));
                }
            } else if sort_column.is_some() {
                planned_outputs.push(Path::new(&output).with_file_name(format!(
                    "{}_정렬_{}.xlsx",
                    Path::new(&output).file_stem().unwrap_or_default().to_string_lossy(),
                    safe_filename_piece(&name)
                )));
            }
        }
    }
    excel::preflight_new_paths(&planned_outputs)?;
    let base = excel::merge_workbooks(&input_paths, Path::new(&output), keep_header, None)?;
    let sheets = excel::inspect(Path::new(&output))?;
    let mut duplicates = 0;
    let mut extra_outputs = Vec::new();
    for (name, mut rows) in sheets {
        if rows.len() > 1 {
            let mut seen = std::collections::HashSet::new();
            duplicates += rows
                .iter()
                .skip(1)
                .filter(|row| {
                    let key = duplicate_column
                        .and_then(|col| row.get(col - 1))
                        .map(|value| value.trim().to_lowercase())
                        .unwrap_or_else(|| format!("row:{row:?}"));
                    !key.is_empty() && !seen.insert(key)
                })
                .count();
        }
        if let Some(col) = sort_column {
            if !rows.is_empty() {
                let header = rows.remove(0);
                rows.sort_by(|a, b| excel::compare_sort_values(a.get(col - 1), b.get(col - 1)));
                rows.insert(0, header);
            }
        }
        if grouping {
            let header = rows.first().cloned().unwrap_or_default();
            let mut groups =
                std::collections::BTreeMap::<(String, String), Vec<Vec<String>>>::new();
            for row in rows.into_iter().skip(1) {
                let grade = grade_column
                    .and_then(|col| row.get(col - 1))
                    .cloned()
                    .unwrap_or_default();
                let class = class_column
                    .and_then(|col| row.get(col - 1))
                    .cloned()
                    .unwrap_or_default();
                groups.entry((grade, class)).or_default().push(row);
            }
            for ((grade, class), data) in groups {
                let mut all = vec![header.clone()];
                all.extend(data);
                let target = roster_group_path(
                    &output,
                    &name,
                    &grade,
                    &class,
                    grade_column.is_some(),
                    class_column.is_some(),
                );
                excel::write_single(&target, &name, &all)?;
                extra_outputs.push(target.display().to_string());
            }
        } else if sort_column.is_some() {
            let mut all = vec![];
            if !rows.is_empty() {
                all.push(rows.remove(0));
            }
            all.extend(rows);
            let target = Path::new(&output).with_file_name(format!(
                "{}_정렬_{}.xlsx",
                Path::new(&output)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy(),
                safe_filename_piece(&name)
            ));
            excel::write_single(&target, &name, &all)?;
            extra_outputs.push(target.display().to_string());
        }
    }
    let mut base = base;
    base.outputs.extend(extra_outputs);
    Ok(RosterResult {
        result: base,
        duplicate_rows: duplicates,
    })
}
fn roster_group_path(
    output: &str,
    sheet: &str,
    grade: &str,
    class: &str,
    include_grade: bool,
    include_class: bool,
) -> PathBuf {
    let mut parts = vec![safe_filename_piece(sheet)];
    if include_grade {
        parts.push(safe_filename_piece(grade));
    }
    if include_class {
        parts.push(safe_filename_piece(class));
    }
    Path::new(output).with_file_name(format!(
        "{}_{}.xlsx",
        Path::new(output)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy(),
        parts.join("_")
    ))
}

fn safe_filename_piece(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect()
}
#[derive(serde::Serialize)]
pub struct RosterResult {
    pub result: ExcelResult,
    pub duplicate_rows: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;
    use tempfile::tempdir;

    fn roster_file(path: &Path, group: &str) {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("Roster").unwrap();
        sheet.write_string(0, 0, "Name").unwrap();
        sheet.write_string(0, 1, "Class").unwrap();
        sheet.write_string(1, 0, "Student").unwrap();
        sheet.write_string(1, 1, group).unwrap();
        workbook.save(path).unwrap();
    }

    #[test]
    fn roster_groups_by_grade_and_class_and_counts_duplicate_student_keys() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("students.xlsx");
        let second = dir.path().join("students-2.xlsx");
        let output = dir.path().join("combined.xlsx");
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("Roster").unwrap();
        for (col, value) in ["Name", "Grade", "Class"].iter().enumerate() {
            sheet.write_string(0, col as u16, *value).unwrap();
        }
        for (row, values) in [["Minji", "1", "A"], ["Junho", "1", "B"], ["Ara", "2", "A"]]
            .iter()
            .enumerate()
        {
            for (col, value) in values.iter().enumerate() {
                sheet
                    .write_string((row + 1) as u32, col as u16, *value)
                    .unwrap();
            }
        }
        workbook.save(&input).unwrap();
        let mut second_book = Workbook::new();
        let second_sheet = second_book.add_worksheet();
        second_sheet.set_name("Roster").unwrap();
        for (col, value) in ["Name", "Grade", "Class"].iter().enumerate() {
            second_sheet.write_string(0, col as u16, *value).unwrap();
        }
        for (col, value) in ["Minji", "1", "A"].iter().enumerate() {
            second_sheet.write_string(1, col as u16, *value).unwrap();
        }
        second_book.save(&second).unwrap();

        let result = excel_roster(
            vec![input.display().to_string(), second.display().to_string()],
            output.display().to_string(),
            Some(2),
            Some(3),
            Some(1),
            None,
            true,
        )
        .unwrap();

        assert_eq!(result.duplicate_rows, 1);
        assert_eq!(result.result.outputs.len(), 4);
        for file in [
            "combined_Roster_1_A.xlsx",
            "combined_Roster_1_B.xlsx",
            "combined_Roster_2_A.xlsx",
        ] {
            assert!(dir.path().join(file).exists(), "missing {file}");
        }
        assert_eq!(
            excel::inspect(&dir.path().join("combined_Roster_1_A.xlsx")).unwrap()[0]
                .1
                .len(),
            3
        );
    }

    #[test]
    fn roster_preflights_grouped_name_collisions_before_writing_base_output() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("first.xlsx");
        let second = dir.path().join("second.xlsx");
        let output = dir.path().join("combined.xlsx");
        roster_file(&first, "A/B");
        roster_file(&second, "A:B");

        assert!(excel_roster(
            vec![first.display().to_string(), second.display().to_string()],
            output.display().to_string(),
            Some(2),
            None,
            None,
            None,
            true,
        )
        .is_err());
        assert!(!output.exists());
    }
}
