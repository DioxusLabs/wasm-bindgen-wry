#!/bin/sh
//usr/bin/env rustc --edition=2024 "$0" -o "${TMPDIR:-/tmp}/bump-wasm-bindgen-version" && BUMP_WASM_BINDGEN_SOURCE="$0" "${TMPDIR:-/tmp}/bump-wasm-bindgen-version" "$@"; exit $?

// Run directly with:
//   ./scripts/bump-wasm-bindgen-version.rs [--suffix alpha.1] [--dry-run|--check]
//
// Or compile manually with:
//   rustc --edition=2024 scripts/bump-wasm-bindgen-version.rs -o /tmp/bump-wasm-bindgen-version
//   /tmp/bump-wasm-bindgen-version [--suffix alpha.1] [--dry-run|--check]

use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

const LOCAL_MANIFESTS: &[(&str, &str)] = &[
    ("wry-bindgen/Cargo.toml", "wry-bindgen"),
    ("wry-bindgen-macro/Cargo.toml", "wry-bindgen-macro"),
    (
        "wry-bindgen-macro-support/Cargo.toml",
        "wry-bindgen-macro-support",
    ),
    ("shims/wasm-bindgen/Cargo.toml", "wasm-bindgen"),
    ("shims/wasm-bindgen-macro/Cargo.toml", "wasm-bindgen-macro"),
];

const LOCAL_CRATES: &[&str] = &[
    "wasm-bindgen",
    "wasm-bindgen-macro",
    "wry-bindgen",
    "wry-bindgen-macro",
    "wry-bindgen-macro-support",
];
const UPSTREAM_PACKAGE_NAMES: &[&str] = &["wasm-bindgen", "not-wasm-bindgen"];
const PATCHED_UPSTREAM_MANIFESTS: &[(&str, &str)] = &[
    ("wasm-bindgen/crates/js-sys/Cargo.toml", "js-sys"),
    ("wasm-bindgen/crates/web-sys/Cargo.toml", "web-sys"),
    (
        "wasm-bindgen/crates/futures/Cargo.toml",
        "wasm-bindgen-futures",
    ),
];
const PINNED_UPSTREAM_DEPENDENCY_MANIFESTS: &[&str] = &["wry-launch/Cargo.toml"];

#[derive(Debug)]
struct Error(String);

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Default)]
struct Args {
    suffix: Option<String>,
    dry_run: bool,
    check: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = parse_args()?;
    let repo_root = repo_root()?;
    let upstream_manifest = repo_root.join("wasm-bindgen/Cargo.toml");
    if !upstream_manifest.exists() {
        return Err(Error::new(format!(
            "missing {}; generate or restore the vendored wasm-bindgen tree first",
            upstream_manifest.display()
        )));
    }

    let base_version = read_package_version(&upstream_manifest, Some(UPSTREAM_PACKAGE_NAMES))?;
    let version = target_version(&base_version, args.suffix.as_deref())?;
    let patched_upstream_versions = read_patched_upstream_versions(&repo_root)?;
    let mut changes = Vec::new();

    for (relative_path, crate_name) in LOCAL_MANIFESTS {
        let path = repo_root.join(relative_path);
        let current = fs::read_to_string(&path)?;
        let updated = update_manifest_text(&path, &current, crate_name, &version)?;
        if updated != current {
            changes.push((path, updated));
        }
    }

    for relative_path in PINNED_UPSTREAM_DEPENDENCY_MANIFESTS {
        let path = repo_root.join(relative_path);
        let current = fs::read_to_string(&path)?;
        let updated =
            update_dependency_versions_text(&current, patched_upstream_versions.as_slice());
        if updated != current {
            changes.push((path, updated));
        }
    }

    let lockfile = repo_root.join("Cargo.lock");
    let current_lock = fs::read_to_string(&lockfile)?;
    let updated_lock = update_lock_text(&lockfile, &current_lock, &version)?;
    if updated_lock != current_lock {
        changes.push((lockfile, updated_lock));
    }

    if args.check {
        if changes.is_empty() {
            println!("local bindgen crate versions already match {version}");
            return Ok(());
        }

        eprintln!("expected local bindgen crate version {version}");
        for (path, _) in &changes {
            eprintln!("needs update: {}", relative_to(&repo_root, path));
        }
        process::exit(1);
    }

    if args.dry_run {
        println!("derived version: {version}");
        if changes.is_empty() {
            println!("no changes needed");
        } else {
            println!("would update:");
            for (path, _) in &changes {
                println!("  {}", relative_to(&repo_root, path));
            }
        }
        return Ok(());
    }

    for (path, updated) in &changes {
        fs::write(path, updated)?;
    }

    if changes.is_empty() {
        println!("local bindgen crate versions already match {version}");
    } else {
        println!("updated local bindgen crate version to {version}:");
        for (path, _) in &changes {
            println!("  {}", relative_to(&repo_root, path));
        }
    }

    Ok(())
}

fn read_patched_upstream_versions(repo_root: &Path) -> Result<Vec<(&'static str, String)>> {
    let mut versions = Vec::new();
    for (relative_path, crate_name) in PATCHED_UPSTREAM_MANIFESTS {
        let path = repo_root.join(relative_path);
        let version = read_package_version(&path, Some(&[*crate_name]))?;
        versions.push((*crate_name, version));
    }
    Ok(versions)
}

fn parse_args() -> Result<Args> {
    let mut args = Args::default();
    let mut raw = env::args().skip(1);

    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            "--suffix" => {
                let suffix = raw
                    .next()
                    .ok_or_else(|| Error::new("--suffix requires a value"))?;
                args.suffix = Some(suffix);
            }
            "--dry-run" => args.dry_run = true,
            "--check" => args.check = true,
            _ => {
                if let Some(suffix) = arg.strip_prefix("--suffix=") {
                    args.suffix = Some(suffix.to_string());
                } else {
                    return Err(Error::new(format!("unknown argument `{arg}`")));
                }
            }
        }
    }

    if args.dry_run && args.check {
        return Err(Error::new("--dry-run and --check cannot be used together"));
    }

    Ok(args)
}

fn print_usage() {
    println!(
        "\
Usage:
  bump-wasm-bindgen-version [--suffix SUFFIX] [--dry-run|--check]

Options:
  --suffix SUFFIX  Append a prerelease suffix such as alpha.1.
  --dry-run        Print files that would change without writing.
  --check          Exit nonzero if files are not already bumped.

The default suffix is none. The upstream version is read from wasm-bindgen/Cargo.toml."
    );
}

fn repo_root() -> Result<PathBuf> {
    if let Some(root) = repo_root_from_path_env()? {
        return Ok(root);
    }

    let script_path = env::current_exe()?;
    if let Some(parent) = script_path.parent() {
        if parent.file_name().is_some_and(|name| name == "scripts") {
            if let Some(root) = parent.parent() {
                return Ok(root.to_path_buf());
            }
        }
    }

    let current_dir = env::current_dir()?;
    for ancestor in current_dir.ancestors() {
        if ancestor.join("Cargo.toml").is_file()
            && ancestor.join("wry-bindgen").is_dir()
            && ancestor.join("shims/wasm-bindgen").is_dir()
        {
            return Ok(ancestor.to_path_buf());
        }
    }

    Err(Error::new(
        "could not find repo root; run from inside wasm-bindgen-wry",
    ))
}

fn repo_root_from_path_env() -> Result<Option<PathBuf>> {
    let Ok(source_path) = env::var("BUMP_WASM_BINDGEN_SOURCE") else {
        return Ok(None);
    };
    let source_path = PathBuf::from(source_path);
    let source_path = if source_path.is_absolute() {
        source_path
    } else {
        env::current_dir()?.join(source_path)
    };

    let Some(parent) = source_path.parent() else {
        return Ok(None);
    };
    if parent.file_name().is_some_and(|name| name == "scripts") {
        return Ok(parent.parent().map(Path::to_path_buf));
    }
    Ok(None)
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn target_version(base_version: &str, suffix: Option<&str>) -> Result<String> {
    let Some(suffix) = suffix else {
        return Ok(base_version.to_string());
    };

    if !is_stable_semver(base_version) {
        return Err(Error::new(format!(
            "cannot append a suffix to non-stable upstream version `{base_version}`"
        )));
    }

    if !is_valid_suffix(suffix) {
        return Err(Error::new(
            "suffix must be non-empty dot-separated prerelease identifiers using only ASCII letters, digits, and hyphens",
        ));
    }

    Ok(format!("{base_version}-{suffix}"))
}

fn is_stable_semver(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_valid_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && suffix.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn read_package_version(path: &Path, expected_names: Option<&[&str]>) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let package = find_section(&text, "package")
        .ok_or_else(|| Error::new(format!("{} is missing a [package] section", path.display())))?;
    let name = read_field(package, "name").ok_or_else(|| {
        Error::new(format!(
            "{} is missing package field `name`",
            path.display()
        ))
    })?;
    if let Some(expected_names) = expected_names {
        if !expected_names.contains(&name) {
            return Err(Error::new(format!(
                "{} package name is `{name}`, expected one of: {}",
                path.display(),
                expected_names.join(", ")
            )));
        }
        if expected_names.contains(&"not-wasm-bindgen") && name != "wasm-bindgen" {
            eprintln!(
                "note: deriving wasm-bindgen version from patched upstream package `{name}` in {}",
                path.display()
            );
        }
    }

    read_field(package, "version")
        .map(str::to_string)
        .ok_or_else(|| {
            Error::new(format!(
                "{} is missing package field `version`",
                path.display()
            ))
        })
}

fn update_manifest_text(
    path: &Path,
    text: &str,
    crate_name: &str,
    target_version: &str,
) -> Result<String> {
    let mut lines = Lines::from(text);
    let (start, end) = lines
        .find_section_bounds("package")
        .ok_or_else(|| Error::new(format!("{} is missing a [package] section", path.display())))?;
    let package_text = lines.range_text(start, end);
    let name = read_field(&package_text, "name").ok_or_else(|| {
        Error::new(format!(
            "{} is missing package field `name`",
            path.display()
        ))
    })?;
    if name != crate_name {
        return Err(Error::new(format!(
            "{} package name is `{name}`, expected `{crate_name}`",
            path.display()
        )));
    }

    let mut updated_package_version = false;
    for index in start..end {
        if lines.replace_field(index, "version", target_version) {
            updated_package_version = true;
            break;
        }
    }
    if !updated_package_version {
        return Err(Error::new(format!(
            "{} is missing package field `version`",
            path.display()
        )));
    }

    for index in 0..lines.len() {
        let line = lines.body(index).to_string();
        let Some((prefix, key, table_body, suffix)) = split_inline_table(&line) else {
            continue;
        };
        let dependency_name = inline_table_value(table_body, "package").unwrap_or(key);
        if !LOCAL_CRATES.contains(&dependency_name) {
            continue;
        }
        if inline_table_value(table_body, "path").is_none() {
            continue;
        }
        if inline_table_value(table_body, "version").is_none() {
            continue;
        }

        let updated_table =
            replace_inline_table_value(table_body, "version", &format!("={target_version}"));
        lines.set_body(index, format!("{prefix}{updated_table}{suffix}"));
    }

    Ok(lines.into_string())
}

fn update_dependency_versions_text(text: &str, dependency_versions: &[(&str, String)]) -> String {
    let mut lines = Lines::from(text);
    for index in 0..lines.len() {
        let line = lines.body(index).to_string();
        if let Some((prefix, key, table_body, suffix)) = split_inline_table(&line) {
            let dependency_name = inline_table_value(table_body, "package").unwrap_or(key);
            let Some(version) = dependency_version(dependency_versions, dependency_name) else {
                continue;
            };
            if inline_table_value(table_body, "version").is_none() {
                continue;
            }

            let updated_table =
                replace_inline_table_value(table_body, "version", &format!("={version}"));
            lines.set_body(index, format!("{prefix}{updated_table}{suffix}"));
            continue;
        }

        for (dependency_name, version) in dependency_versions {
            if lines.replace_field(index, dependency_name, &format!("={version}")) {
                break;
            }
        }
    }
    lines.into_string()
}

fn update_lock_text(path: &Path, text: &str, target_version: &str) -> Result<String> {
    let mut lines = Lines::from(text);
    let mut found = BTreeSet::new();
    let mut index = 0;

    while index < lines.len() {
        if lines.body(index).trim() != "[[package]]" {
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < lines.len() && lines.body(index).trim() != "[[package]]" {
            index += 1;
        }
        let end = index;
        let block = lines.range_text(start, end);
        let Some(name) = read_field(&block, "name") else {
            continue;
        };
        if !LOCAL_CRATES.contains(&name) || read_field(&block, "source").is_some() {
            continue;
        }

        found.insert(name.to_string());
        replace_package_version(&mut lines, path, start, end, name, target_version)?;
    }

    let missing: Vec<_> = LOCAL_CRATES
        .iter()
        .filter(|crate_name| !found.contains(**crate_name))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(Error::new(format!(
            "{} is missing local package entries for: {}",
            path.display(),
            missing.join(", ")
        )));
    }

    Ok(lines.into_string())
}

fn dependency_version<'a>(
    dependency_versions: &'a [(&str, String)],
    dependency_name: &str,
) -> Option<&'a str> {
    dependency_versions
        .iter()
        .find_map(|(name, version)| (*name == dependency_name).then_some(version.as_str()))
}

fn replace_package_version(
    lines: &mut Lines,
    path: &Path,
    start: usize,
    end: usize,
    name: &str,
    version: &str,
) -> Result<()> {
    for line_index in start..end {
        if lines.replace_field(line_index, "version", version) {
            return Ok(());
        }
    }
    Err(Error::new(format!(
        "{} package `{name}` is missing field `version`",
        path.display()
    )))
}

fn find_section<'a>(text: &'a str, section: &str) -> Option<&'a str> {
    let mut start = None;
    for (index, line) in text.lines().enumerate() {
        if let Some(name) = section_name(line) {
            if let Some(start) = start {
                return Some(slice_lines(text, start, index));
            }
            if name == section {
                start = Some(index + 1);
            }
        }
    }
    start.map(|start| slice_lines(text, start, text.lines().count()))
}

fn section_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    let name = trimmed.trim_start_matches('[').trim_end_matches(']');
    if name.starts_with('[') || name.ends_with(']') {
        return None;
    }
    Some(name)
}

fn slice_lines(text: &str, start_line: usize, end_line: usize) -> &str {
    let mut start_byte = text.len();
    let mut end_byte = text.len();
    let mut line = 0;

    for (byte_index, _) in text.match_indices('\n') {
        if line == start_line.saturating_sub(1) {
            start_byte = byte_index + 1;
        }
        if line == end_line.saturating_sub(1) {
            end_byte = byte_index;
            break;
        }
        line += 1;
    }

    if start_line == 0 {
        start_byte = 0;
    }
    if start_byte == text.len() && start_line == text.lines().count() {
        start_byte = text.len();
    }

    &text[start_byte..end_byte]
}

fn read_field<'a>(text: &'a str, field: &str) -> Option<&'a str> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = strip_toml_field_prefix(trimmed, field) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let end = rest.find('"')?;
        return Some(&rest[..end]);
    }
    None
}

fn strip_toml_field_prefix<'a>(trimmed_line: &'a str, field: &str) -> Option<&'a str> {
    let rest = trimmed_line.strip_prefix(field)?;
    let next = rest.chars().next();
    if next.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
        return None;
    }
    let rest = rest.trim_start();
    rest.strip_prefix('=')
}

fn split_inline_table(line: &str) -> Option<(&str, &str, &str, &str)> {
    let equals = line.find('=')?;
    let key = line[..equals].trim();
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }

    let after_equals = &line[equals + 1..];
    let open = after_equals.find('{')?;
    if !after_equals[..open].trim().is_empty() {
        return None;
    }

    let close = line.rfind('}')?;
    let table_start = equals + 1 + open;
    if close <= table_start {
        return None;
    }

    Some((
        &line[..=table_start],
        key,
        &line[table_start + 1..close],
        &line[close..],
    ))
}

fn inline_table_value<'a>(table: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("{key}");
    let mut offset = 0;
    while let Some(found) = table[offset..].find(&pattern) {
        let start = offset + found;
        let before = table[..start].chars().next_back();
        let after = table[start + pattern.len()..].chars().next();
        let valid_before =
            before.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'));
        let valid_after =
            after.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'));
        if valid_before && valid_after {
            let rest = table[start + pattern.len()..].trim_start();
            let rest = rest.strip_prefix('=')?.trim_start();
            let rest = rest.strip_prefix('"')?;
            let end = rest.find('"')?;
            return Some(&rest[..end]);
        }
        offset = start + pattern.len();
    }
    None
}

fn replace_inline_table_value(table: &str, key: &str, value: &str) -> String {
    let pattern = format!("{key}");
    let mut offset = 0;
    while let Some(found) = table[offset..].find(&pattern) {
        let start = offset + found;
        let before = table[..start].chars().next_back();
        let after = table[start + pattern.len()..].chars().next();
        let valid_before =
            before.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'));
        let valid_after =
            after.is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'));
        if valid_before && valid_after {
            let mut cursor = start + pattern.len();
            cursor += table[cursor..].find('=').expect("key has an equals sign") + 1;
            cursor += table[cursor..]
                .find('"')
                .expect("value starts with a quote")
                + 1;
            let end = cursor + table[cursor..].find('"').expect("value ends with a quote");

            let mut updated = String::new();
            updated.push_str(&table[..cursor]);
            updated.push_str(value);
            updated.push_str(&table[end..]);
            return updated;
        }
        offset = start + pattern.len();
    }
    table.to_string()
}

struct Lines {
    lines: Vec<Line>,
}

struct Line {
    body: String,
    ending: &'static str,
}

impl Lines {
    fn from(text: &str) -> Self {
        let mut lines = Vec::new();
        for chunk in text.split_inclusive('\n') {
            if let Some(body) = chunk.strip_suffix("\r\n") {
                lines.push(Line {
                    body: body.to_string(),
                    ending: "\r\n",
                });
            } else if let Some(body) = chunk.strip_suffix('\n') {
                lines.push(Line {
                    body: body.to_string(),
                    ending: "\n",
                });
            } else {
                lines.push(Line {
                    body: chunk.to_string(),
                    ending: "",
                });
            }
        }
        if text.is_empty() {
            lines.clear();
        }
        Self { lines }
    }

    fn len(&self) -> usize {
        self.lines.len()
    }

    fn body(&self, index: usize) -> &str {
        &self.lines[index].body
    }

    fn set_body(&mut self, index: usize, body: String) {
        self.lines[index].body = body;
    }

    fn find_section_bounds(&self, section: &str) -> Option<(usize, usize)> {
        let mut start = None;
        for (index, line) in self.lines.iter().enumerate() {
            if let Some(name) = section_name(&line.body) {
                if let Some(start) = start {
                    return Some((start, index));
                }
                if name == section {
                    start = Some(index + 1);
                }
            }
        }
        start.map(|start| (start, self.lines.len()))
    }

    fn range_text(&self, start: usize, end: usize) -> String {
        self.lines[start..end]
            .iter()
            .map(|line| {
                let mut text = line.body.clone();
                text.push_str(line.ending);
                text
            })
            .collect()
    }

    fn replace_field(&mut self, index: usize, field: &str, value: &str) -> bool {
        let trimmed = self.lines[index].body.trim_start();
        let whitespace_len = self.lines[index].body.len() - trimmed.len();
        let Some(rest) = trimmed.strip_prefix(field) else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('=') {
            return false;
        }
        let Some(first_quote) = self.lines[index].body.find('"') else {
            return false;
        };
        let value_start = first_quote + 1;
        let Some(value_end) = self.lines[index].body[value_start..].find('"') else {
            return false;
        };
        let value_end = value_start + value_end;

        let mut updated = String::new();
        updated.push_str(&self.lines[index].body[..value_start]);
        updated.push_str(value);
        updated.push_str(&self.lines[index].body[value_end..]);
        if whitespace_len <= self.lines[index].body.len() {
            self.lines[index].body = updated;
            return true;
        }
        false
    }

    fn into_string(self) -> String {
        let mut text = String::new();
        for line in self.lines {
            text.push_str(&line.body);
            text.push_str(line.ending);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_version_defaults_to_base() {
        assert_eq!(target_version("0.2.122", None).unwrap(), "0.2.122");
    }

    #[test]
    fn target_version_appends_suffix() {
        assert_eq!(
            target_version("0.2.122", Some("alpha.1")).unwrap(),
            "0.2.122-alpha.1"
        );
    }

    #[test]
    fn manifest_update_changes_package_and_path_dependency_versions() {
        let input = r#"[package]
name = "wry-bindgen"
version = "0.2.106-alpha.1"

[dependencies]
wry-bindgen-macro = { path = "../wry-bindgen-macro", version = "=0.2.106-alpha.1" }
serde = "1"
"#;
        let output =
            update_manifest_text(Path::new("Cargo.toml"), input, "wry-bindgen", "0.2.122").unwrap();

        assert!(output.contains("version = \"0.2.122\""));
        assert!(output.contains("version = \"=0.2.122\""));
        assert!(output.contains("serde = \"1\""));
    }

    #[test]
    fn lock_update_changes_only_local_packages() {
        let input = r#"[[package]]
name = "wasm-bindgen"
version = "0.2.122"
dependencies = []

[[package]]
name = "wasm-bindgen"
version = "0.2.122"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "wasm-bindgen-macro"
version = "0.2.122"

[[package]]
name = "wry-bindgen"
version = "0.2.106-alpha.1"

[[package]]
name = "wry-bindgen-macro"
version = "0.2.106-alpha.1"

[[package]]
name = "wry-bindgen-macro-support"
version = "0.2.106-alpha.1"
"#;
        let output = update_lock_text(Path::new("Cargo.lock"), input, "0.2.122-alpha.1").unwrap();

        assert!(output.contains("version = \"0.2.122-alpha.1\""));
        assert!(
            output.contains("source = \"registry+https://github.com/rust-lang/crates.io-index\"")
        );
    }

    #[test]
    fn dependency_update_pins_patched_upstream_crates() {
        let input = r#"[dev-dependencies]
web-sys = { version = "0.3.98", features = ["Window"] }
wasm-bindgen-futures = "0.4.71"
serde = "1"
"#;
        let versions = vec![
            ("web-sys", "0.3.99".to_string()),
            ("wasm-bindgen-futures", "0.4.72".to_string()),
        ];
        let output = update_dependency_versions_text(input, &versions);

        assert!(output.contains("web-sys = { version = \"=0.3.99\""));
        assert!(output.contains("wasm-bindgen-futures = \"=0.4.72\""));
        assert!(output.contains("serde = \"1\""));
    }

    #[test]
    fn upstream_package_names_include_patched_name() {
        assert!(UPSTREAM_PACKAGE_NAMES.contains(&"wasm-bindgen"));
        assert!(UPSTREAM_PACKAGE_NAMES.contains(&"not-wasm-bindgen"));
    }
}
