//! The staged install / uninstall runners. Each stage marks itself running →
//! done, and a failure marks that stage failed before bubbling up, so the UI
//! always shows *where* it stopped.

use super::ipc::{InstallOptions, log, stage};
use super::{install, setup, uninstall, wire};
use tauri::AppHandle;

pub(crate) async fn run_stages(app: &AppHandle, options: &InstallOptions) -> Result<(), String> {
    log(
        app,
        format!(
            "target={} · add_to_path={} · desktop_shortcut={}",
            options.install_dir, options.add_to_path, options.desktop_shortcut
        ),
    );

    stage(app, "core", "running");
    install::core(app, options).await.inspect_err(|_| {
        stage(app, "core", "failed");
    })?;
    stage(app, "core", "done");

    stage(app, "app", "running");
    install::app_files(app, options).await.inspect_err(|_| {
        stage(app, "app", "failed");
    })?;
    stage(app, "app", "done");

    stage(app, "wire", "running");
    wire::run(app, options).inspect_err(|_| {
        stage(app, "wire", "failed");
    })?;
    stage(app, "wire", "done");

    // Regent is in place, so Setup's own unpacked files are dead weight from
    // here on. Outside the stages deliberately: this cannot fail an install
    // that has already succeeded. On a failure we never get here, which is
    // what we want — the payload stays put for a retry.
    setup::discard(app, &options.install_dir);

    Ok(())
}

pub(crate) async fn run_uninstall_stages(app: &AppHandle) -> Result<(), String> {
    let dir = uninstall::install_dir()?;
    log(app, format!("removing {}", dir.display()));
    log(app, "your ~/.regent data will be left untouched".into());

    stage(app, "app", "running");
    uninstall::stop_processes(app)
        .and_then(|()| uninstall::remove_dir(app, &dir, "app"))
        .inspect_err(|_| stage(app, "app", "failed"))?;
    stage(app, "app", "done");

    stage(app, "core", "running");
    uninstall::remove_dir(app, &dir, "bin").inspect_err(|_| stage(app, "core", "failed"))?;
    stage(app, "core", "done");

    // Last: unwire, then schedule the directory (including this .exe) to go.
    stage(app, "wire", "running");
    uninstall::unwire(app, &dir)
        .and_then(|()| uninstall::schedule_self_delete(app, &dir))
        .inspect_err(|_| stage(app, "wire", "failed"))?;
    stage(app, "wire", "done");

    Ok(())
}
