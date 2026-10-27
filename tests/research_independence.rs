//! The research feature must stay independent of the thesis feature (plan §3 #10, §9, M1 #8):
//! no thesis/vault vocabulary and no thesis imports under `src/research` or the web feature
//! folder.

use std::path::{Path, PathBuf};

// R5-FIX1 (2026-10-24): canon "Dossier" is the name of the ticker page (design.md 4.2), so the bare
// word is legitimate UI vocabulary. What the gate forbids is the v4 `dossier.rs` coupling, so
// `dossier` is matched only in its module/import forms.
const FORBIDDEN: [&str; 6] = [
    "thesis",
    "vault",
    "falsif",
    "dossier.rs",
    "mod dossier",
    "dossier::",
];

/// First forbidden pattern on a line, if any: a substring above, or a `use` of a `dossier` path.
fn forbidden_in(line: &str) -> Option<&'static str> {
    let lower = line.to_lowercase();
    if let Some(word) = FORBIDDEN.iter().find(|w| lower.contains(*w)) {
        return Some(word);
    }
    let t = lower.trim_start();
    let t = t.strip_prefix("pub ").unwrap_or(t);
    if t.starts_with("use ") && t.contains("dossier") {
        return Some("use ..dossier");
    }
    None
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn violations(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    files_under(root, &mut files);
    let mut found = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if let Some(word) = forbidden_in(line) {
                found.push(format!("{}:{}: `{word}`", f.display(), i + 1));
            }
        }
    }
    found
}

#[test]
fn research_sources_do_not_mention_or_import_the_thesis_feature() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("src/research");
    let mut scanned = Vec::new();
    files_under(&src, &mut scanned);
    assert!(scanned.len() >= 5, "scan must actually see src/research");

    let mut found = violations(&src);
    // The web folder arrives with a later lane; scan it as soon as it exists.
    found.extend(violations(&root.join("web/src/features/research")));
    assert!(found.is_empty(), "thesis coupling:\n{}", found.join("\n"));
}

#[test]
fn the_scan_catches_a_planted_violation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "use crate::Thesis;\n").unwrap();
    std::fs::write(dir.path().join("b.rs"), "fn ok() {}\n").unwrap();
    assert_eq!(violations(dir.path()).len(), 1);
}

#[test]
fn the_scan_flags_v4_dossier_coupling_but_not_canon_ui_vocabulary() {
    for bad in [
        "mod dossier;",
        "use crate::research::dossier;",
        "use super::dossier::build;",
        "dossier::build()",
        "// see dossier.rs",
    ] {
        assert!(forbidden_in(bad).is_some(), "should flag: {bad}");
    }
    for ok in [
        "const TAB = \"dossier\";",
        "<h1>Dossier</h1>",
        "import { Dossier } from \"./Dossier\";",
    ] {
        assert!(forbidden_in(ok).is_none(), "should not flag: {ok}");
    }
}
