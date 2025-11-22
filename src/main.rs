#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::Instant,
};

use slint::{Model, ModelRc, SharedString, VecModel};
use walkdir::{DirEntry, WalkDir};
use wildmatch::WildMatch;

slint::include_modules!();

fn main() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;

    // 初始化 UI 状态
    ui.set_org_enabled(true);
    ui.set_html_enabled(true);
    ui.set_dsstore_enabled(true);
    ui.set_action_text("扫描".into());
    ui.set_status_message("准备就绪".into());

    // 获取 home 目录，如果获取失败则使用当前目录
    let home_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    // 初始化显示 Home 目录
    update_ui_path(&ui, &home_path);

    // --- 事件处理：点击文件夹列表进入子目录 ---
    let ui_weak = ui.as_weak();
    ui.on_folder_clicked(move |folder_name| {
        let ui = ui_weak.unwrap();
        if ui.get_is_processing() {
            return;
        }

        let mut path = reconstruct_path(&ui);
        path.push(folder_name.as_str());

        if path.exists() {
            update_ui_path(&ui, &path);
        }
    });

    // --- 事件处理：点击面包屑导航跳转 ---
    let ui_weak = ui.as_weak();
    ui.on_path_part_clicked(move |index| {
        let ui = ui_weak.unwrap();
        if ui.get_is_processing() {
            return;
        }

        let parts = ui.get_current_path_parts();
        let mut new_path = std::path::PathBuf::new();

        // 重新构建路径直到点击的索引
        // 注意：Linux下第一个可能是 "/"，Windows下可能是 "C:"
        for i in 0..=index {
            let v = parts.row_data(i.try_into().unwrap()).unwrap().to_string();
            new_path.push(v);
        }

        if new_path.exists() && new_path.is_dir() {
            update_ui_path(&ui, &new_path);
        }
    });

    // --- 事件处理：扫描/清理按钮 ---
    let ui_weak = ui.as_weak();
    ui.on_action_clicked(move || {
        let ui = ui_weak.unwrap();
        if ui.get_is_processing() {
            return;
        }

        let current_action = ui.get_action_text().to_string();
        let path = reconstruct_path(&ui);

        if !path.exists() {
            ui.set_status_message("错误：当前路径不存在".into());
            return;
        }

        // 获取配置的文件类型
        let patterns = collect_patterns(&ui);
        if patterns.is_empty() && current_action == "扫描" {
            ui.set_status_message("提示：请先选择至少一种文件类型".into());
            return;
        }

        // 1. 设置 UI 为忙碌状态
        ui.set_is_processing(true);
        ui.set_status_message(
            if current_action == "扫描" {
                "正在扫描..."
            } else {
                "正在清理..."
            }
            .into(),
        );
        // UI 进入加载状态
        ui.set_action_text("处理中...".into());

        // 2. 收集要清理的文件列表（仅在清理模式下需要）
        let files_to_delete: Vec<String> = if current_action == "清理" {
            let model = ui.get_scan_results();
            (0..model.row_count())
                .flat_map(|i| model.row_data(i))
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        };

        let ui_weak_thread = ui_weak.clone();

        // 3. 在后台线程执行耗时操作
        thread::spawn(move || {
            if current_action == "扫描" {
                let start = Instant::now();
                // 扫描操作，可能很耗时
                let found_files = scan_files(&path, &patterns);
                let count = found_files.len();
                let duration = start.elapsed();

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
                        if count > 0 {
                            ui.set_action_text("清理".into());
                            ui.set_status_message(
                                format!("扫描完成，耗时 {:.2?}，发现 {} 个文件", duration, count)
                                    .into(),
                            );
                        } else {
                            ui.set_action_text("扫描".into());
                            ui.set_status_message("扫描完成，未发现匹配文件".into());
                        }
                        ui.set_is_processing(false);
                    }
                });
            } else if current_action == "清理" {
                let mut success_count = 0;
                let mut fail_count = 0;

                for f_path in files_to_delete {
                    match fs::remove_file(&f_path) {
                        Ok(_) => success_count += 1,
                        Err(e) => {
                            eprintln!("Failed to delete {}: {}", f_path, e);
                            fail_count += 1;
                        }
                    }
                }

                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui_weak_thread.upgrade() {
                        // 清空列表
                        ui.set_scan_results(ModelRc::from(Rc::new(VecModel::from(vec![]))));
                        ui.set_action_text("扫描".into());
                        ui.set_status_message(
                            format!(
                                "清理完成：成功 {} 个，失败 {} 个",
                                success_count, fail_count
                            )
                            .into(),
                        );
                        ui.set_is_processing(false);
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
    ui.set_status_message("就绪".into());
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
 * 判断是否是应该忽略的目录
 */
fn is_ignored(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| {
            s.starts_with('.')
                || s == "node_modules"
                || s == "target"
                || s == "dist"
                || s == "build"
        })
        .unwrap_or(false)
}

/**
 * 扫描目录中的文件
 */
fn scan_files(dir: &Path, patterns: &[&str]) -> Vec<String> {
    let mut found = vec![];
    let walker = WalkDir::new(dir).into_iter();
    for entry in walker.filter_entry(|e| !is_ignored(e)).flatten() {
        if entry.file_type().is_file() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            for p in patterns {
                if WildMatch::new(p).matches(&file_name) {
                    found.push(entry.path().to_string_lossy().into_owned());
                    break; // 匹配到一个规则即可
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
