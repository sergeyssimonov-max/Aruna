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
/// is a build with no WebDriver server in it – and the test compiles under
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
