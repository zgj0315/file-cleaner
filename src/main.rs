#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};
use walkdir::WalkDir;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;
    ui.set_selected_directory("".into());
    ui.set_org_enabled(true);
    ui.set_html_enabled(true);
    ui.set_dsstore_enabled(true);
    ui.set_action_text("Scan".into());

    let ui_weak = ui.as_weak();
    ui.on_choose_dir(move || {
        let ui = ui_weak.unwrap();
        let home_path = std::env::var("HOME").unwrap();
        if let Some(dir) = rfd::FileDialog::new()
            .set_directory(home_path)
            .pick_folder()
        {
            ui.set_selected_directory(dir.to_string_lossy().to_string().into());
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
    }
}
