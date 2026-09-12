#[test]
fn service_files_respect_the_500_line_file_limit() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/service");
    let mut total = 0usize;
    let mut per_file = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let lines = std::fs::read_to_string(&path).unwrap().lines().count();
        per_file.push(format!(
            "{}: {lines}",
            path.file_name().unwrap().to_string_lossy()
        ));
        total += lines;
    }
    per_file.sort();
    let over: Vec<&String> = per_file
        .iter()
        .filter(|f| f.rsplit(' ').next().unwrap().parse::<usize>().unwrap() > 500)
        .collect();
    assert!(
        over.is_empty(),
        "service files over the 500-line limit ({total} total):\n{}",
        per_file
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn service_modules_contain_no_coordination_words() {
    let banned = [
        "lock",
        "lease",
        "fence",
        "generation",
        "epoch",
        "cursor",
        "claim",
        "pin",
        "coordinator",
        "broker",
        "journal",
        "repair",
        "continuation",
        "handoff",
    ];
    let allowlist = [(std::path::Path::new("src/service/status.rs"), "lock")];
    let singleton_lock_lines = [
        "pub(crate) fn acquire_service_lock",
        "join(\"service.lock\")",
        "file.try_lock()",
        "could not acquire service lock",
        "let _service_lock = acquire_service_lock",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("src/service"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("rs"))
        .collect();
    files.push(root.join("src/tools/workspace/indexing/seed.rs"));
    files.push(root.join("src/tools/workspace/commands/registry/status.rs"));
    let mut hits = Vec::new();
    let mut allowed_hits = 0;
    for path in files {
        let relative_path = path.strip_prefix(root).unwrap();
        let fname = relative_path.to_string_lossy();
        for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            let lower = line.to_lowercase();
            for word in banned {
                if lower
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|t| t == word)
                {
                    if allowlist.contains(&(relative_path, word)) && allowed_hits == 0 {
                        allowed_hits += 1;
                        continue;
                    }
                    if relative_path == std::path::Path::new("src/service/mod.rs")
                        && word == "lock"
                        && singleton_lock_lines
                            .iter()
                            .any(|allowed| line.contains(allowed))
                    {
                        continue;
                    }
                    hits.push(format!("{fname}:{}: {word}", n + 1));
                }
            }
        }
    }
    assert_eq!(
        allowed_hits, 1,
        "expected exactly 1 allowlisted lock() call in src/service/status.rs"
    );
    assert!(
        hits.is_empty(),
        "coordination words in service modules:\n{}",
        hits.join("\n")
    );
}
