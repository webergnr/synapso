//! The CI scripts stay ASCII, and this is why.
//!
//! Windows PowerShell 5.1 reads a `.ps1` with no BOM using the system ANSI
//! codepage, and GitHub's runner writes every `run:` block as UTF-8. An em
//! dash is `E2 80 94`; byte `94` in CP1252 is U+201D, the closing smart quote
//! — and PowerShell accepts smart quotes as string delimiters. So a comment
//! with an em dash in it silently opens a string, swallows the rest of the
//! script, and the parser reports "The string is missing the terminator"
//! against a line dozens of rows further down.
//!
//! Two `v0.2.0` release builds died on exactly that: compiled, tested, zipped,
//! then failed on the last step. This test is here so the third one cannot.
//!
//! ⚠️ the check is on the BYTES a `powershell` step will be handed, not on
//! this file — this file is Rust, read as UTF-8, and may say `—` freely.

use std::path::Path;

/// The characters Windows PowerShell will treat as a double-quote delimiter.
const QUOTES: [char; 3] = ['"', '\u{201C}', '\u{201D}'];

/// What PowerShell 5.1 sees: the UTF-8 bytes, decoded as CP1252.
///
/// Only the high range matters and only three of those bytes can become a
/// quote, so this maps the ones that can rather than carrying a full table.
fn as_cp1252(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            0x93 => '\u{201C}',
            0x94 => '\u{201D}',
            _ => b as char,
        })
        .collect()
}

/// Every `run:` block under a `shell: powershell` job, with its step name.
fn powershell_runs(yaml: &str) -> Vec<(String, String)> {
    // ⚠️ a hand-rolled scan and not a YAML crate: this test must not add a
    // dependency to the workspace to read two files whose shape it controls.
    if !yaml.contains("shell: powershell") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut name, mut body, mut inside) = (String::from("?"), String::new(), false);
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("- name: ") {
            if inside {
                out.push((std::mem::take(&mut name), std::mem::take(&mut body)));
                inside = false;
            }
            name = rest.to_string();
        }
        if trimmed == "run: |" || trimmed.starts_with("run: |") {
            inside = true;
            body.clear();
            continue;
        }
        if inside {
            // the block ends at the next line that is not part of it
            if !line.is_empty() && !line.starts_with("          ") {
                out.push((name.clone(), std::mem::take(&mut body)));
                inside = false;
            } else {
                body.push_str(line);
                body.push('\n');
            }
        }
    }
    if inside {
        out.push((name, body));
    }
    out
}

#[test]
fn no_powershell_step_hides_a_quote_in_its_bytes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("no .github/workflows") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "yml") {
            continue;
        }
        let yaml = std::fs::read_to_string(&path).expect("read the workflow");
        for (step, run) in powershell_runs(&yaml) {
            checked += 1;
            let seen = as_cp1252(&run);
            let smart: Vec<char> = seen
                .chars()
                .filter(|c| QUOTES.contains(c) && *c != '"')
                .collect();
            assert!(
                smart.is_empty(),
                "{}: step {step:?} carries {} byte(s) that PowerShell reads as a quote. \
                 Some non-ASCII character in it decodes to a smart quote under CP1252 — \
                 an em dash does. Write it in ASCII.",
                path.display(),
                smart.len()
            );
        }
    }
    assert!(
        checked >= 2,
        "found only {checked} powershell steps — the scan stopped matching the workflows"
    );
}
