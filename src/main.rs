#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
};

use slint::{Model, ModelRc, SharedString, VecModel};
use walkdir::WalkDir;
use wildmatch::WildMatch;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;

    // 初始化 UI 状态
    ui.set_org_enabled(true);
    ui.set_html_enabled(true);
    ui.set_dsstore_enabled(true);
    ui.set_action_text("扫描".into());

    // 获取 home 目录
    let home_path = dirs::home_dir().ok_or(anyhow::anyhow!("无法找到 home 目录"))?;

    update_ui_path(&ui, &home_path);

    // 路径点击
    let ui_weak = ui.as_weak();
    ui.on_folder_clicked(move |folder_name| {
        let ui = ui_weak.unwrap();
        let mut path = reconstruct_path(&ui);
        path.push(folder_name.as_str());
        if path.exists() {
            update_ui_path(&ui, &path);
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
            update_ui_path(&ui, &new_path);
        }
    });

    // 扫描与清理
    let ui_weak = ui.as_weak();
    ui.on_action_clicked(move || {
        let ui = ui_weak.unwrap();

        let current_action = ui.get_action_text().to_string();
        let path = reconstruct_path(&ui);

        if !path.exists() {
            eprintln!("未选择目录");
            return;
        }

        // 获取配置的文件类型
        let patterns = collect_patterns(&ui);

        // UI 进入加载状态
        ui.set_action_text("处理中...".into());

        let files_to_delete: Vec<String> = if current_action == "清理" {
            let model = ui.get_scan_results();
            (0..model.row_count())
                .map(|i| model.row_data(i).unwrap().to_string())
                .collect()
        } else {
            Vec::new()
        };

        let ui_weak_thread = ui_weak.clone();
        thread::spawn(move || {
            if current_action == "扫描" {
                // 扫描操作，可能很耗时
                let found_files = scan_files(&path, &patterns);

                // 回到主线程更新 UI
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak_thread.upgrade() {
                        let model = Rc::new(VecModel::from(
                            found_files
                                .into_iter()
                                .map(SharedString::from)
                                .collect::<Vec<_>>(),
                        ));
                        ui.set_scan_results(ModelRc::from(model));
                        ui.set_action_text("清理".into());
                    }
                });
            } else if current_action == "清理" {
                // 在清理前最好再次扫描或复用上一次的结果（为了安全，这里简化为重扫并删）
                // 实际生产中应该只删除列表中显示的文件

                for f in files_to_delete {
                    println!("delete {f:?}");
                    let _ = std::fs::remove_file(f);
                }

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak_thread.upgrade() {
                        // 清空列表
                        ui.set_scan_results(ModelRc::from(Rc::new(VecModel::from(vec![]))));
                        ui.set_action_text("扫描".into());
                    }
                });
            }
        });
    });
    ui.run()?;
    Ok(())
}

/**
 * UI 更新逻辑
 */
fn update_ui_path(ui: &AppWindow, path: &Path) {
    ui.set_current_path_parts(path_to_parts(path));
    ui.set_folder_list(list_folders(path));
    ui.set_action_text("扫描".into());
    // 清空旧的扫描结果
    ui.set_scan_results(ModelRc::from(Rc::new(VecModel::from(vec![]))));
}

/**
 * 路径重组逻辑
 */
fn reconstruct_path(ui: &AppWindow) -> PathBuf {
    let parts = ui.get_current_path_parts();
    let mut path = PathBuf::new();
    for i in 0..parts.row_count() {
        let v = parts.row_data(i).unwrap().to_string();
        path.push(v);
    }
    path
}

/**
 * 文件类型
 */
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
/**
 * 扫描目录中的文件
 */
fn scan_files(dir: &Path, patterns: &[&str]) -> Vec<String> {
    let mut found = vec![];

    for entry in WalkDir::new(dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                for p in patterns {
                    if WildMatch::new(p).matches(&file_name) {
                        found.push(entry.path().to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    found
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
/**
 * 拆分路径
 * 将路径 /Users/username/Downloads 拆分为 ["Users","username","Downloads"]
 */
fn path_to_parts(path: &Path) -> ModelRc<SharedString> {
    let mut parts = vec![];
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        parts.push(SharedString::from(s.into_owned()));
    }

    ModelRc::new(VecModel::from(parts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_parts_test() {
        // Linux
        let path = Path::new("/home/zhaogj/Download");
        let model = path_to_parts(path);
        let actual: Vec<String> = (0..model.row_count())
            .map(|i| model.row_data(i).unwrap().to_string())
            .collect();

        let expected = vec!["/", "home", "zhaogj", "Download"];
        assert_eq!(actual, expected, "路径拆分结果与预期不符");
    }
}
