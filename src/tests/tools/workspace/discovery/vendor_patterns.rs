//! Vendor-pattern detection coverage.

use super::*;

#[test]
fn test_analyze_vendor_patterns_does_not_flag_libs_directory() {
    // libs/ is the standard source directory in Nx/Angular monorepos (apps/ + libs/).
    // It must NOT be treated as vendor — same reasoning as packages/ for npm/pnpm.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "libs/shared/src/index.ts",
        "libs/shared/src/lib.ts",
        "libs/ui/src/button.ts",
        "libs/ui/src/input.ts",
        "libs/ui/src/modal.ts",
        "libs/data-access/src/store.ts",
        "libs/data-access/src/effects.ts",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns
            .iter()
            .any(|p| p == "libs" || p.starts_with("libs/")),
        "libs/ must NOT be flagged as vendor — it's the standard source dir in Nx/Angular monorepos. Got: {:?}",
        patterns,
    );
}

#[test]
fn test_analyze_vendor_patterns_does_not_flag_bin_directory() {
    // bin/ overwhelmingly holds user CLI scripts in modern repos
    // (npm package bin entries, install scripts, executable utilities).
    // Compiled output is already caught by target/, dist/, build/, out/.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "bin/install.js",
        "bin/cli.js",
        "bin/run.js",
        "bin/deploy.js",
        "bin/setup.js",
        "bin/migrate.js",
        "bin/lint.js", // >5 files, would trip the count threshold
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns.iter().any(|p| p == "bin" || p.starts_with("bin/")),
        "bin/ must NOT be flagged as vendor — it overwhelmingly holds user CLI scripts. Got: {:?}",
        patterns,
    );
}

#[test]
fn test_analyze_vendor_patterns_does_not_flag_plugin_directory() {
    // plugin/plugins commonly hold user-authored code in plugin monorepos
    // (julie-plugin, codex-plugin-cc) and CMS plugin directories. Third-party
    // plugins should live under vendor/ or third-party/ when applicable.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/plugin/plugin1.js",
        "Scripts/plugin/plugin2.js",
        "Scripts/plugin/plugin3.js",
        "Scripts/plugin/plugin4.js",
        "Scripts/plugin/plugin5.js",
        "Scripts/plugin/plugin6.js", // >5 files
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns
            .iter()
            .any(|p| p == "Scripts/plugin" || p == "plugin" || p == "plugins"),
        "plugin/ must NOT be auto-flagged — user-authored plugin code lives here. Got: {:?}",
        patterns,
    );
}

#[test]
fn test_analyze_vendor_patterns_detects_vendor_directory() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "vendor/lib1.js",
        "vendor/lib2.js",
        "vendor/lib3.js",
        "vendor/lib4.js",
        "vendor/lib5.js",
        "vendor/lib6.js", // >5 files
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0], "vendor");
}

#[test]
fn test_analyze_vendor_patterns_detects_jquery_files() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/jquery-1.12.4.js",
        "Scripts/jquery-ui.js",
        "Scripts/jquery.validate.js",
        "Scripts/jquery.unobtrusive-ajax.js", // >3 jquery files triggers detection
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        1,
        "Should detect directory with >3 jquery files"
    );
    assert_eq!(patterns[0], "Scripts");
}

#[test]
fn test_analyze_vendor_patterns_detects_bootstrap_files() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Styles/bootstrap.css",
        "Styles/bootstrap-theme.css",
        "Styles/bootstrap.min.css", // >2 bootstrap files triggers detection
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        1,
        "Should detect directory with >2 bootstrap files"
    );
    assert_eq!(patterns[0], "Styles");
}

#[test]
fn test_analyze_vendor_patterns_ignores_few_jquery_files() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/jquery.js",
        "Scripts/jquery-ui.js", // Only 2 jquery files, needs >3
        "Scripts/custom.js",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        0,
        "Should NOT detect with only 2 jquery files"
    );
}

#[test]
fn test_analyze_vendor_patterns_detects_minified_concentration() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "dist/app.min.js",
        "dist/vendor.min.js",
        "dist/styles.min.css",
        "dist/bootstrap.min.css",
        "dist/jquery.min.js",
        "dist/angular.min.js",
        "dist/lodash.min.js",
        "dist/moment.min.js",
        "dist/react.min.js",
        "dist/vue.min.js",
        "dist/axios.min.js", // 11 minified files (>10)
        "dist/config.js",    // 12 total files, 11/12 = 91% (>50%)
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        1,
        "Should detect high minified concentration"
    );
    assert_eq!(patterns[0], "dist");
}

#[test]
fn test_analyze_vendor_patterns_ignores_low_minified_concentration() {
    let tool = create_tool();
    // Use "compiled" instead of "build" since "build" is now a recognized vendor directory
    let (temp_dir, files) = create_workspace_with_files(vec![
        "compiled/app.min.js",
        "compiled/vendor.min.js",
        "compiled/styles.min.css", // 3 minified files (needs >10)
        "compiled/source1.js",
        "compiled/source2.js",
        "compiled/source3.js",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        0,
        "Should NOT detect with <10 minified files"
    );
}

#[test]
fn test_analyze_vendor_patterns_ignores_minified_below_50_percent() {
    let tool = create_tool();
    // Use "compiled" instead of "build" since "build" is now a recognized vendor directory
    let (temp_dir, files) = create_workspace_with_files(vec![
        "compiled/app.min.js",
        "compiled/vendor.min.js",
        "compiled/styles.min.css",
        "compiled/bootstrap.min.css",
        "compiled/jquery.min.js",
        "compiled/angular.min.js",
        "compiled/lodash.min.js",
        "compiled/moment.min.js",
        "compiled/react.min.js",
        "compiled/vue.min.js",
        "compiled/axios.min.js", // 11 minified files (>10) ✓
        // But add 11+ non-minified files to drop below 50%
        "compiled/source1.js",
        "compiled/source2.js",
        "compiled/source3.js",
        "compiled/source4.js",
        "compiled/source5.js",
        "compiled/source6.js",
        "compiled/source7.js",
        "compiled/source8.js",
        "compiled/source9.js",
        "compiled/source10.js",
        "compiled/source11.js",
        "compiled/source12.js", // 23 total, 11/23 = 47% (<50%)
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect when minified <50%");
}

#[test]
fn test_analyze_vendor_patterns_detects_multiple_directories() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        // First vendor directory (vendor/)
        "Scripts/vendor/lib1.js",
        "Scripts/vendor/lib2.js",
        "Scripts/vendor/lib3.js",
        "Scripts/vendor/lib4.js",
        "Scripts/vendor/lib5.js",
        "Scripts/vendor/lib6.js",
        // Second vendor directory (third-party/)
        "Scripts/third-party/p1.js",
        "Scripts/third-party/p2.js",
        "Scripts/third-party/p3.js",
        "Scripts/third-party/p4.js",
        "Scripts/third-party/p5.js",
        "Scripts/third-party/p6.js",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 2, "Should detect 2 vendor directories");
    assert!(patterns.contains(&"Scripts/vendor".to_string()));
    assert!(patterns.contains(&"Scripts/third-party".to_string()));
}

#[test]
fn test_analyze_vendor_patterns_no_false_positives_for_normal_code() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "src/components/UserService.ts",
        "src/components/AuthService.ts",
        "src/components/PaymentService.ts",
        "src/utils/helpers.ts",
        "src/utils/validators.ts",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect normal source code");
}

#[test]
fn test_analyze_vendor_patterns_does_not_flag_lib_directory() {
    // lib/ is a primary source directory in Elixir, Ruby, Dart, and Haskell.
    // It must NOT be flagged as vendor code.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "lib/my_app/router.ex",
        "lib/my_app/endpoint.ex",
        "lib/my_app/channel.ex",
        "lib/my_app/controller.ex",
        "lib/my_app/views/page.ex",
        "lib/my_app/views/layout.ex",
        "lib/my_app/views/error.ex",
        "lib/my_app/application.ex",
        "lib/my_app.ex",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns.iter().any(|p| p == "lib" || p.starts_with("lib/")),
        "lib/ must NOT be flagged as vendor — it's a source directory in Elixir/Ruby/Dart. Got: {:?}",
        patterns,
    );
}

#[test]
fn test_analyze_vendor_patterns_does_not_flag_packages_directory() {
    // packages/ is the standard monorepo layout for npm/pnpm workspaces,
    // Lerna, Nx, and Turborepo projects. It contains actual source code,
    // not vendor/third-party code. Must NOT be flagged as vendor.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "packages/zod/src/v4/core/api.ts",
        "packages/zod/src/v4/core/checks.ts",
        "packages/zod/src/v4/core/parse.ts",
        "packages/zod/src/v4/core/schemas.ts",
        "packages/zod/src/v4/core/errors.ts",
        "packages/zod/src/v4/core/util.ts",
        "packages/zod/src/v4/core/core.ts",
        "packages/zod/src/v4/mini/parse.ts",
        "packages/docs/src/pages/index.tsx",
        "packages/docs/src/pages/docs.tsx",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns
            .iter()
            .any(|p| p == "packages" || p.starts_with("packages/")),
        "packages/ must NOT be flagged as vendor — it's the standard JS/TS monorepo source layout. Got: {:?}",
        patterns,
    );
}
