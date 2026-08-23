#![forbid(unsafe_code)]

#[cfg(feature = "e2e")]
use tauri::Manager;

#[cfg(feature = "e2e")]
fn wdio_webdriver_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_wdio_webdriver::init()
}

#[cfg(not(feature = "e2e"))]
fn wdio_webdriver_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("noop-wdio-webdriver").build()
}

#[cfg(feature = "e2e")]
fn wdio_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_wdio::init()
}

#[cfg(not(feature = "e2e"))]
fn wdio_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("noop-wdio").build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(wdio_webdriver_plugin())
        .plugin(wdio_plugin())
        .setup(|app| {
            #[cfg(feature = "e2e")]
            app.handle()
                .add_capability(include_str!("../capabilities-e2e/e2e.json"))?;

            #[cfg(not(feature = "e2e"))]
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Gated on the feature as well as on `test`: the one test inside is about what
// a build *without* `e2e` registers, so under the feature the module would be
// empty and its imports unused — which `clippy --all-features` reports,
// correctly.
#[cfg(all(test, not(feature = "e2e")))]
mod tests {
    use super::*;
    use tauri::plugin::Plugin as _;

    /// **The release build carries no WebDriver, and the compiler is what says
    /// so.**
    ///
    /// Four things keep the end-to-end contour out of a release, and three of
    /// them are checked in `frontend/tests/spec-guard.test.ts` by reading the
    /// files that declare them: the two wdio crates are `optional`, they are
    /// reached only through the `e2e` feature, and their permissions live
    /// outside the directory `build.rs` scans.
    ///
    /// This is the fourth, and it is the only one that is not a reading of a
    /// declaration: without the feature, the plugins the builder actually
    /// registers are the no-ops declared above. An optional dependency that is
    /// not selected is never compiled, so a build that reaches this assertion
    /// is a build with no WebDriver server in it — and the test compiles under
    /// exactly the feature set a release uses, which is the default one.
    ///
    /// Under `--features e2e` it does not run at all: there the answer is
    /// supposed to be different, and asserting it here would only restate the
    /// `cfg` a few lines above.
    #[test]
    fn a_build_without_the_feature_registers_no_webdriver() {
        let webdriver = wdio_webdriver_plugin::<tauri::Wry>();
        let backend = wdio_plugin::<tauri::Wry>();

        assert_eq!(
            webdriver.name(),
            "noop-wdio-webdriver",
            "a release build registered a real WebDriver server"
        );
        assert_eq!(
            backend.name(),
            "noop-wdio",
            "a release build registered the wdio backend"
        );
    }
}
