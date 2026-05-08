use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub fn default_plugin_root(workspace_root: &Path) -> PathBuf {
    workspace_root
        .parent()
        .map(|p| p.join("julie-plugin"))
        .unwrap_or_else(|| PathBuf::from("../julie-plugin"))
}

pub fn run_sync_plugin(
    workspace_root: &Path,
    plugin_root: &Path,
    dry_run: bool,
    out: &mut impl Write,
) -> Result<SyncReport> {
    let source_skills = workspace_root.join(".claude").join("skills");
    let plugin_skills = plugin_root.join("skills");
    let source_hooks = workspace_root.join(".claude").join("hooks");
    let plugin_hooks = plugin_root.join("hooks");

    for (label, path) in [
        ("source skills", &source_skills),
        ("plugin skills", &plugin_skills),
        ("source hooks", &source_hooks),
        ("plugin hooks", &plugin_hooks),
    ] {
        if !path.is_dir() {
            bail!("{label} dir not found: {}", path.display());
        }
    }

    writeln!(
        out,
        "sync-plugin: {} → {}{}",
        workspace_root.display(),
        plugin_root.display(),
        if dry_run { " (dry run)" } else { "" }
    )?;

    let mut report = SyncReport::default();
    report.dry_run = dry_run;

    sync_skills(&source_skills, &plugin_skills, dry_run, out, &mut report)?;
    diff_hooks(&source_hooks, &plugin_hooks, out, &mut report)?;

    writeln!(
        out,
        "\nsummary: skills {} updated, {} unchanged, {} removed; hooks divergent (report-only): {} differ, {} source-only, {} plugin-only",
        report.skills_updated.len(),
        report.skills_unchanged.len(),
        report.skills_removed.len(),
        report.hooks_differ.len(),
        report.hooks_source_only.len(),
        report.hooks_plugin_only.len(),
    )?;

    Ok(report)
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub dry_run: bool,
    pub skills_updated: Vec<PathBuf>,
    pub skills_unchanged: Vec<PathBuf>,
    pub skills_removed: Vec<PathBuf>,
    pub hooks_differ: Vec<PathBuf>,
    pub hooks_identical: Vec<PathBuf>,
    pub hooks_source_only: Vec<PathBuf>,
    pub hooks_plugin_only: Vec<PathBuf>,
}

fn sync_skills(
    source: &Path,
    plugin: &Path,
    dry_run: bool,
    out: &mut impl Write,
    report: &mut SyncReport,
) -> Result<()> {
    writeln!(out, "\n[skills] source → plugin (full mirror)")?;

    let source_files = collect_files(source)?;
    let plugin_files = collect_files(plugin)?;

    for rel in &source_files {
        let src = source.join(rel);
        let dst = plugin.join(rel);
        if files_equal(&src, &dst)? {
            writeln!(out, "  = {}", rel.display())?;
            report.skills_unchanged.push(rel.clone());
        } else {
            if !dry_run {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("create_dir_all {}", parent.display())
                    })?;
                }
                fs::copy(&src, &dst).with_context(|| {
                    format!("copy {} → {}", src.display(), dst.display())
                })?;
            }
            writeln!(out, "  → {}", rel.display())?;
            report.skills_updated.push(rel.clone());
        }
    }

    for rel in plugin_files.difference(&source_files) {
        let dst = plugin.join(rel);
        if !dry_run {
            fs::remove_file(&dst)
                .with_context(|| format!("remove {}", dst.display()))?;
        }
        writeln!(out, "  - {} (removed; not in source)", rel.display())?;
        report.skills_removed.push(rel.clone());
    }

    if !dry_run {
        prune_empty_dirs(plugin)?;
    }

    Ok(())
}

fn diff_hooks(
    source: &Path,
    plugin: &Path,
    out: &mut impl Write,
    report: &mut SyncReport,
) -> Result<()> {
    writeln!(out, "\n[hooks] report-only (intentionally separate per CLAUDE.md)")?;
    writeln!(
        out,
        "  source uses relative `node .claude/hooks/...` paths;"
    )?;
    writeln!(
        out,
        "  plugin uses `${{CLAUDE_PLUGIN_ROOT}}` and a plugin-only lib/. They diverge by design."
    )?;

    let source_files = collect_files(source)?;
    let plugin_files = collect_files(plugin)?;

    let shared: BTreeSet<PathBuf> = source_files
        .intersection(&plugin_files)
        .cloned()
        .collect();

    for rel in &shared {
        let src = source.join(rel);
        let dst = plugin.join(rel);
        if files_equal(&src, &dst)? {
            writeln!(out, "  = {}", rel.display())?;
            report.hooks_identical.push(rel.clone());
        } else {
            writeln!(out, "  ≠ {} (differs; reconcile manually if needed)", rel.display())?;
            report.hooks_differ.push(rel.clone());
        }
    }

    let source_only: Vec<PathBuf> = source_files.difference(&plugin_files).cloned().collect();
    if !source_only.is_empty() {
        writeln!(out, "  source-only:")?;
        for rel in &source_only {
            writeln!(out, "    {}", rel.display())?;
        }
        report.hooks_source_only = source_only;
    }

    let plugin_only: Vec<PathBuf> = plugin_files.difference(&source_files).cloned().collect();
    if !plugin_only.is_empty() {
        writeln!(out, "  plugin-only:")?;
        for rel in &plugin_only {
            writeln!(out, "    {}", rel.display())?;
        }
        report.hooks_plugin_only = plugin_only;
    }

    Ok(())
}

fn collect_files(dir: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    walk(dir, dir, &mut files)?;
    Ok(files)
}

fn walk(root: &Path, current: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(current).with_context(|| format!("read_dir {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk(root, &path, out)?;
        } else if ft.is_file() {
            let rel = path
                .strip_prefix(root)
                .with_context(|| format!("strip_prefix {} from {}", root.display(), path.display()))?
                .to_path_buf();
            out.insert(rel);
        }
    }
    Ok(())
}

fn files_equal(a: &Path, b: &Path) -> Result<bool> {
    if !a.exists() || !b.exists() {
        return Ok(false);
    }
    let a_bytes = fs::read(a).with_context(|| format!("read {}", a.display()))?;
    let b_bytes = fs::read(b).with_context(|| format!("read {}", b.display()))?;
    Ok(a_bytes == b_bytes)
}

fn prune_empty_dirs(root: &Path) -> Result<()> {
    let mut entries: Vec<PathBuf> = Vec::new();
    collect_dirs(root, &mut entries)?;
    entries.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for dir in entries {
        if dir == root {
            continue;
        }
        if fs::read_dir(&dir)?.next().is_none() {
            fs::remove_dir(&dir)
                .with_context(|| format!("remove_dir {}", dir.display()))?;
        }
    }
    Ok(())
}

fn collect_dirs(current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    out.push(current.to_path_buf());
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_dirs(&entry.path(), out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(p: &Path, content: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    fn make_layout() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("julie");
        let plugin = tmp.path().join("julie-plugin");

        // Source skills
        write(
            &workspace.join(".claude/skills/editing/SKILL.md"),
            "source-editing-v2",
        );
        write(
            &workspace.join(".claude/skills/editing/references/note.md"),
            "extra-skill-asset",
        );
        write(
            &workspace.join(".claude/skills/explore-area/SKILL.md"),
            "source-explore",
        );

        // Source hooks
        write(
            &workspace.join(".claude/hooks/hooks.json"),
            "{\"dev\": true}",
        );
        write(
            &workspace.join(".claude/hooks/pretool-edit.cjs"),
            "// source pretool-edit (stale)",
        );
        write(
            &workspace.join(".claude/hooks/probe.cjs"),
            "// source-only debug probe",
        );

        // Plugin skills
        write(
            &plugin.join("skills/editing/SKILL.md"),
            "plugin-editing-v1-old",
        );
        write(
            &plugin.join("skills/editing/stale.md"),
            "plugin-only-skill-file-should-be-removed",
        );
        write(&plugin.join("skills/explore-area/SKILL.md"), "source-explore");

        // Plugin hooks
        write(
            &plugin.join("hooks/hooks.json"),
            "{\"distributed\": true}",
        );
        write(
            &plugin.join("hooks/pretool-edit.cjs"),
            "// plugin pretool-edit (canonical)",
        );
        write(
            &plugin.join("hooks/codex-pretooluse.cjs"),
            "// plugin-only codex hook",
        );
        write(&plugin.join("hooks/lib/format.cjs"), "// plugin-only lib");

        (tmp, workspace, plugin)
    }

    #[test]
    fn sync_plugin_tests_skills_mirror_source_to_plugin() {
        let (_tmp, workspace, plugin) = make_layout();
        let mut buf = Vec::new();
        let report = run_sync_plugin(&workspace, &plugin, false, &mut buf).unwrap();

        // editing/SKILL.md updated
        let editing = fs::read_to_string(plugin.join("skills/editing/SKILL.md")).unwrap();
        assert_eq!(editing, "source-editing-v2");

        // references/note.md added
        let note = fs::read_to_string(plugin.join("skills/editing/references/note.md")).unwrap();
        assert_eq!(note, "extra-skill-asset");

        // explore-area/SKILL.md unchanged (identical bytes)
        let explore = fs::read_to_string(plugin.join("skills/explore-area/SKILL.md")).unwrap();
        assert_eq!(explore, "source-explore");

        // stale.md removed
        assert!(!plugin.join("skills/editing/stale.md").exists());

        let updated_paths: Vec<_> = report
            .skills_updated
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(updated_paths.contains(&"editing/SKILL.md".to_string()));
        assert!(updated_paths.contains(&"editing/references/note.md".to_string()));
        let removed_paths: Vec<_> = report
            .skills_removed
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(removed_paths, vec!["editing/stale.md".to_string()]);
    }

    #[test]
    fn sync_plugin_tests_hooks_are_report_only_no_writes() {
        let (_tmp, workspace, plugin) = make_layout();
        let mut buf = Vec::new();
        let report = run_sync_plugin(&workspace, &plugin, false, &mut buf).unwrap();

        // Shared files: NEITHER side modified
        let source_hooks_json =
            fs::read_to_string(workspace.join(".claude/hooks/hooks.json")).unwrap();
        assert_eq!(source_hooks_json, "{\"dev\": true}");
        let plugin_hooks_json =
            fs::read_to_string(plugin.join("hooks/hooks.json")).unwrap();
        assert_eq!(plugin_hooks_json, "{\"distributed\": true}");

        let source_pretool =
            fs::read_to_string(workspace.join(".claude/hooks/pretool-edit.cjs")).unwrap();
        assert_eq!(source_pretool, "// source pretool-edit (stale)");
        let plugin_pretool =
            fs::read_to_string(plugin.join("hooks/pretool-edit.cjs")).unwrap();
        assert_eq!(plugin_pretool, "// plugin pretool-edit (canonical)");

        // Unique files preserved on each side
        assert!(workspace.join(".claude/hooks/probe.cjs").exists());
        assert!(!plugin.join("hooks/probe.cjs").exists());
        assert!(plugin.join("hooks/codex-pretooluse.cjs").exists());
        assert!(!workspace.join(".claude/hooks/codex-pretooluse.cjs").exists());

        // Report classifies divergence
        let differ: Vec<_> = report
            .hooks_differ
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(differ.contains(&"hooks.json".to_string()));
        assert!(differ.contains(&"pretool-edit.cjs".to_string()));

        let source_only: Vec<_> = report
            .hooks_source_only
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(source_only, vec!["probe.cjs".to_string()]);

        let plugin_only: Vec<_> = report
            .hooks_plugin_only
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(plugin_only.contains(&"codex-pretooluse.cjs".to_string()));
        assert!(plugin_only.contains(&"lib/format.cjs".to_string()));
    }

    #[test]
    fn sync_plugin_tests_dry_run_does_not_modify_files() {
        let (_tmp, workspace, plugin) = make_layout();
        let mut buf = Vec::new();
        let report = run_sync_plugin(&workspace, &plugin, true, &mut buf).unwrap();

        // editing/SKILL.md unchanged on disk
        let editing = fs::read_to_string(plugin.join("skills/editing/SKILL.md")).unwrap();
        assert_eq!(editing, "plugin-editing-v1-old");

        // stale.md still present (would be removed in real run)
        assert!(plugin.join("skills/editing/stale.md").exists());

        // Report still shows planned skill updates and hook divergence
        assert!(!report.skills_updated.is_empty());
        assert!(!report.hooks_differ.is_empty());
        assert!(report.dry_run);
    }

    #[test]
    fn sync_plugin_tests_idempotent_after_first_run() {
        let (_tmp, workspace, plugin) = make_layout();
        let mut buf1 = Vec::new();
        run_sync_plugin(&workspace, &plugin, false, &mut buf1).unwrap();

        let mut buf2 = Vec::new();
        let report = run_sync_plugin(&workspace, &plugin, false, &mut buf2).unwrap();

        assert!(
            report.skills_updated.is_empty(),
            "second run should report no skill changes, got {:?}",
            report.skills_updated
        );
        assert!(report.skills_removed.is_empty());
        // Hooks remain divergent (report-only) — not an error
        assert!(!report.hooks_differ.is_empty() || !report.hooks_source_only.is_empty()
            || !report.hooks_plugin_only.is_empty());
    }

    #[test]
    fn sync_plugin_tests_default_plugin_root_is_sibling() {
        let workspace = PathBuf::from("/Users/test/source/julie");
        let plugin_root = default_plugin_root(&workspace);
        assert_eq!(plugin_root, PathBuf::from("/Users/test/source/julie-plugin"));
    }
}
