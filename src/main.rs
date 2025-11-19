#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{fs, path::Path, rc::Rc};

use slint::{Model, ModelRc, SharedString, VecModel};
use walkdir::WalkDir;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;
    ui.set_selected_directory("".into());
    ui.set_org_enabled(true);
    ui.set_html_enabled(true);
    ui.set_dsstore_enabled(true);
    ui.set_action_text("Scan".into());

    // 设置初始路径和文件夹列表
    let home_path_str = std::env::var("HOME").unwrap();
    let home_path = Path::new(&home_path_str);

    ui.set_current_path_parts(path_to_parts(&home_path));
    ui.set_folder_list(list_folders(&home_path));

    // 点击路径层处理
    let ui_weak = ui.as_weak();
    ui.on_folder_clicked(move |folder_name| {
        let ui = ui_weak.unwrap();

        let parts = ui.get_current_path_parts();
        let mut new_path = std::path::PathBuf::new();

        // 先构造当前路径
        for i in 0..parts.row_count() {
            let v = parts.row_data(i).unwrap().to_string();
            new_path.push(v);
        }

        // 点击的子文件夹
        new_path.push(folder_name);

        // 刷新 UI
        if new_path.exists() {
            ui.set_current_path_parts(path_to_parts(&new_path));
            ui.set_folder_list(list_folders(&new_path));
            ui.set_action_text("Scan".into());
        }
    });
    // 点击文件夹处理
    let ui_weak = ui.as_weak();
    ui.on_path_part_clicked(move |index| {
        let ui = ui_weak.unwrap();

        let parts = ui.get_current_path_parts();
        let mut new_path = std::path::PathBuf::new();

        for i in 0..=index {
            let v = parts.row_data(i.try_into().unwrap()).unwrap().to_string();
            new_path.push(v);
        }

        if new_path.exists() {
            ui.set_current_path_parts(path_to_parts(&new_path));
            ui.set_folder_list(list_folders(&new_path));
            ui.set_action_text("Scan".into());
        }
    });

    let ui_weak = ui.as_weak();
    ui.on_action_clicked(move || {
        let ui = ui_weak.unwrap();

        let current_action = ui.get_action_text().to_string();
        let dir = ui.get_selected_directory().to_string();

        if dir.is_empty() {
            eprintln!("未选择目录");
            return;
        }

        let patterns = collect_patterns(&ui);

        if current_action == "Scan" {
            println!("开始扫描: {dir}");
            let found_files = scan_files(&dir, &patterns);
            println!("扫描发现 {} 个垃圾文件", found_files.len());
            let model = Rc::new(VecModel::from(
                found_files
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<SharedString>>(),
            ));

            ui.set_scan_results(ModelRc::from(model));
            ui.set_action_text("Clean".into());
        } else {
            println!("开始清理: {dir}");
            let found_files = scan_files(&dir, &patterns);

            for f in found_files {
                println!("delete {f}");
                let _ = std::fs::remove_file(&f);
            }
            println!("清理完成");

            ui.set_action_text("Scan".into());
        }
    });
    ui.run()?;
    Ok(())
}

fn collect_patterns(ui: &AppWindow) -> Vec<&'static str> {
    let mut p = vec![];
    if ui.get_org_enabled() {
        p.push("*.org~");
    }
    if ui.get_html_enabled() {
        p.push("*.html~");
    }
    if ui.get_dsstore_enabled() {
        p.push(".DS_Store");
    }
    p
}

fn scan_files(dir: &str, patterns: &[&str]) -> Vec<String> {
    let mut found = vec![];

    for entry in WalkDir::new(dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                for p in patterns {
                    if pattern_match(&name, p) {
                        found.push(entry.path().to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    found
}

fn pattern_match(file: &str, pat: &str) -> bool {
    if pat.ends_with('*') {
        // * 号结尾的情况
        let ext = &pat[..(pat.len() - 1)];
        return file.starts_with(ext);
    } else if pat.starts_with('*') {
        // * 号开头的情况
        let ext = &pat[1..];
        return file.ends_with(ext);
    } else {
        return file.eq(pat);
    }
}
fn list_folders(path: &Path) -> ModelRc<SharedString> {
    let mut folders = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    folders.push(SharedString::from(
                        entry.file_name().to_string_lossy().as_ref(),
                    ));
                }
            }
        }
    }
    folders.sort();
    ModelRc::new(VecModel::from(folders))
}

// 将路径拆分为 ["Users","username","Downloads"]
fn path_to_parts(path: &Path) -> ModelRc<SharedString> {
    let mut parts = vec![SharedString::from("/")];
    for comp in path.iter() {
        if let Some(s) = comp.to_str() {
            if s != "/" {
                parts.push(SharedString::from(s.to_string()));
            }
        }
    }
    ModelRc::new(VecModel::from(parts))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pattern_match_test() {
        let file_name = "abc.org~";
        let pat = "*.org~";
        assert!(pattern_match(file_name, pat));
        let pat = "abc.*";
        assert!(pattern_match(file_name, pat));
        let file_name = ".DS_Store";
        let pat = ".DS_Store";
        assert!(pattern_match(file_name, pat));
    }
}
