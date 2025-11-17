#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
        if let Some(dir) = rfd::FileDialog::new().set_directory(".").pick_folder() {
            ui.set_selected_directory(dir.to_string_lossy().to_string().into());
            ui.set_action_text("Scan".into());
        }
    });
    ui.run()?;
    Ok(())
}
