mod commands;
mod excel;
mod images;
mod output;
mod pdf;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            workflows::workflow_load,
            workflows::workflow_save,
            workflows::workflow_run,
            commands::excel::excel_inspect,
            commands::excel::excel_merge,
            commands::excel::excel_split,
            commands::excel::excel_clean,
            commands::excel::excel_to_csv,
            commands::excel::csv_to_excel,
            commands::excel::excel_roster,
            commands::files::preview_rename,
            commands::files::apply_rename,
            commands::files::scan_duplicates,
            commands::files::list_files,
            commands::files::preview_organize_files,
            commands::files::organize_files,
            commands::files::audit_vocabulary_images,
            commands::files::export_csv,
            commands::files::export_xlsx,
            commands::files::search_files,
            images::convert_images,
            pdf::pdf_merge,
            pdf::pdf_extract,
            pdf::pdf_split,
            pdf::pdf_images,
            pdf::pdf_parity,
            pdf::pdf_rotate,
            pdf::pdf_delete,
            pdf::pdf_reorder,
            phase6::convert_document,
            phase6::privacy_scan,
            phase6::privacy_export_csv
        ])
        .run(tauri::generate_context!())
        .expect("EduWork Tools 실행 실패");
}

mod phase6;

mod workflows;
