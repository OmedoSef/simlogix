//! Writes `THIRD-PARTY.md`: what SimLogix depends on, and the licence each
//! dependency is offered under.
//!
//! # Why a generated file rather than a live lookup
//!
//! The obligation these licences carry is *attribution*, and attribution has
//! to travel with the thing being distributed. A file generated here, checked
//! in, and embedded into the binary is one that reaches whoever ends up with
//! a copy — a lookup against a registry the user doesn't have would not.
//!
//! Run it after changing a dependency:
//!
//! ```text
//! cargo run -p simlogix-gui --bin write-licenses -- THIRD-PARTY.md
//! ```
//!
//! # What it collects
//!
//! Only what is actually *shipped*: the normal dependency graph reachable
//! from this workspace's own crates. Dev-dependencies build the tests and
//! build-dependencies run at build time; neither ends up in a released
//! binary, so neither is something a user receives.
//!
//! Licence texts are deduplicated by content. Apache-2.0 is byte-identical
//! wherever it appears and would otherwise be repeated a hundred times, while
//! MIT differs only in its copyright line and genuinely has to be repeated —
//! that line *is* the attribution.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

/// One dependency, as the notice file lists it.
struct Crate {
    name: String,
    version: String,
    license: String,
    repository: Option<String>,
    /// Absolute path to the crate's own source directory.
    root: PathBuf,
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: write-licenses <output path>");
        std::process::exit(2);
    });

    let crates = match collect() {
        Ok(crates) => crates,
        Err(message) => {
            eprintln!("could not read the dependency graph: {message}");
            std::process::exit(1);
        }
    };

    let notice = render(&crates);
    if let Err(error) = std::fs::write(&path, &notice) {
        eprintln!("could not write {path}: {error}");
        std::process::exit(1);
    }
    println!(
        "wrote {path} — {} dependencies, {} KiB",
        crates.len(),
        notice.len() / 1024
    );
}

/// Reads `cargo metadata` and walks the normal dependency graph out from this
/// workspace's own crates.
fn collect() -> Result<Vec<Crate>, String> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1"])
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;

    let packages = metadata["packages"]
        .as_array()
        .ok_or("no packages in the metadata")?;
    let by_id: HashMap<&str, &serde_json::Value> = packages
        .iter()
        .filter_map(|package| Some((package["id"].as_str()?, package)))
        .collect();

    // Normal dependencies only — see the module docs.
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in metadata["resolve"]["nodes"]
        .as_array()
        .ok_or("no resolve graph")?
    {
        let id = node["id"].as_str().unwrap_or_default();
        let mut reached = Vec::new();
        for dep in node["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| kind["kind"].is_null());
            if normal {
                if let Some(pkg) = dep["pkg"].as_str() {
                    reached.push(pkg);
                }
            }
        }
        edges.insert(id, reached);
    }

    let ours: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .ok_or("no workspace members")?
        .iter()
        .filter_map(|id| id.as_str())
        .collect();

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&str> = ours.iter().copied().collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        stack.extend(edges.get(id).into_iter().flatten().copied());
    }

    let mut crates: Vec<Crate> = seen
        .into_iter()
        .filter(|id| !ours.contains(id))
        .filter_map(|id| {
            let package = by_id.get(id)?;
            let manifest = Path::new(package["manifest_path"].as_str()?);
            Some(Crate {
                name: package["name"].as_str()?.to_string(),
                version: package["version"].as_str()?.to_string(),
                // A crate with no declared licence is a problem to be looked
                // at, not one to be papered over with a guess.
                license: package["license"]
                    .as_str()
                    .unwrap_or("NOT DECLARED")
                    .to_string(),
                repository: package["repository"].as_str().map(str::to_string),
                root: manifest.parent()?.to_path_buf(),
            })
        })
        .collect();
    crates.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.version.cmp(&b.version))
    });
    Ok(crates)
}

/// The licence files a crate ships, in a stable order.
fn licence_files(root: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let upper = name.to_uppercase();
            let wanted = upper.starts_with("LICENSE")
                || upper.starts_with("LICENCE")
                || upper.starts_with("COPYING")
                || upper.starts_with("NOTICE");
            // Read as lossy text: a licence file is text, and one stray byte
            // is no reason to drop an attribution on the floor.
            wanted
                .then(|| std::fs::read(entry.path()).ok())
                .flatten()
                .map(|bytes| (name, String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect();
    found.sort();
    found
}

fn render(crates: &[Crate]) -> String {
    let mut out = String::new();
    out.push_str(
        "# Third-party licences\n\n\
         SimLogix itself is offered under the MIT licence — see [LICENSE](LICENSE).\n\n\
         This file lists everything it is built on, and the terms each of those \
         is offered under. It is **generated**; run\n\n\
         ```bash\n\
         cargo run -p simlogix-gui --bin write-licenses -- THIRD-PARTY.md\n\
         ```\n\n\
         after changing a dependency rather than editing it by hand.\n\n\
         Only shipped dependencies are listed: dev-dependencies build the \
         tests and build-dependencies run at build time, and neither reaches \
         a released binary.\n\n",
    );

    out.push_str(&format!("## Dependencies ({})\n\n", crates.len()));
    out.push_str("| Crate | Version | Licence |\n|---|---|---|\n");
    for entry in crates {
        let name = match &entry.repository {
            Some(url) => format!("[{}]({url})", entry.name),
            None => entry.name.clone(),
        };
        out.push_str(&format!(
            "| {name} | {} | {} |\n",
            entry.version, entry.license
        ));
    }

    // Grouped by the exact text, so a licence shared by a hundred crates is
    // printed once with its hundred names above it.
    let mut texts: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut without: Vec<&str> = Vec::new();
    let owned: Vec<(String, Vec<(String, String)>)> = crates
        .iter()
        .map(|entry| {
            (
                format!("{} {}", entry.name, entry.version),
                licence_files(&entry.root),
            )
        })
        .collect();
    for (who, files) in &owned {
        if files.is_empty() {
            without.push(who);
            continue;
        }
        for (file, text) in files {
            texts
                .entry(text.as_str())
                .or_default()
                .push(format!("{who} ({file})"));
        }
    }

    out.push_str("\n## Licence texts\n\n");
    out.push_str(
        "Identical texts are shown once, with every crate that ships them \
         listed above. A crate's own copyright line is part of its \
         attribution, which is why the MIT text appears many times over.\n",
    );
    for (text, users) in &texts {
        out.push_str("\n---\n\n");
        for who in users {
            out.push_str(&format!("- {who}\n"));
        }
        out.push_str("\n```\n");
        out.push_str(text.trim_end());
        out.push_str("\n```\n");
    }

    if !without.is_empty() {
        out.push_str(
            "\n---\n\n## Crates shipping no licence file\n\n\
             These declare their terms in their manifest — listed in the table \
             above — but ship no copy of the text alongside their source.\n\n",
        );
        for who in without {
            out.push_str(&format!("- {who}\n"));
        }
    }

    out
}
