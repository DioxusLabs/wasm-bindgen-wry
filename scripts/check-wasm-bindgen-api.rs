#!/bin/sh
//usr/bin/env rustc --edition=2024 "$0" -o "${TMPDIR:-/tmp}/check-wasm-bindgen-api" && CHECK_WASM_BINDGEN_API_SOURCE="$0" "${TMPDIR:-/tmp}/check-wasm-bindgen-api" "$@"; exit $?

// Run directly with:
//   ./scripts/check-wasm-bindgen-api.rs [wasm-bindgen-ref]
//
// Or compile manually with:
//   rustc --edition=2024 scripts/check-wasm-bindgen-api.rs -o /tmp/check-wasm-bindgen-api
//   /tmp/check-wasm-bindgen-api [wasm-bindgen-ref]

use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

// The public API lives in `wry-bindgen` (the desktop runtime crate). The published
// `wasm-bindgen` shim is a thin facade that only re-exports from `wry-bindgen`, and
// cargo-semver-checks (rustdoc JSON) cannot see items through cross-crate re-exports,
// so diffing the shim reports its entire surface as "missing". We instead diff
// `wry-bindgen` directly and rename the baseline package to match (see
// `rename_baseline_package`) so the public paths line up (`wry_bindgen::JsValue` on
// both sides).
const PACKAGE: &str = "wry-bindgen";
const CURRENT_MANIFEST: &str = "wry-bindgen/Cargo.toml";
const BASELINE_PACKAGE_NAME: &str = "wry-bindgen";
const UPSTREAM_REMOTE: &str = "https://github.com/wasm-bindgen/wasm-bindgen.git";

// Findings the script treats as known intentional desktop-vs-wasm differences and removes
// from the report (see `filter_ignored_failures`). The lints stay active, so other findings
// of the same kind still fail; this lives in the script because cargo-semver-checks supports
// configuration only per lint.
//
// wasm-ABI / codegen marker traits from upstream's `convert`/`describe` modules, internal to
// wasm codegen. A `derive_trait_impl_removed` finding for one of these is ignored; any other
// derived trait (e.g. `Clone`, `Debug`) still fails.
const IGNORED_DERIVE_TRAITS: &[&str] = &[
    "IntoWasmAbi",
    "OptionIntoWasmAbi",
    "FromWasmAbi",
    "OptionFromWasmAbi",
    "RefFromWasmAbi",
    "LongRefFromWasmAbi",
    "WasmDescribe",
    "UpcastFrom",
    "IntoJsGeneric",
    "Promising",
];

// Types built through a constructor with private fields (vs upstream's doc-hidden public
// field). A `constructible_struct_adds_private_field` finding for one of these is ignored.
const IGNORED_CONSTRUCTIBLE_TYPES: &[&str] = &["JsThreadLocal"];

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

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Default)]
struct Args {
    upstream_ref: Option<String>,
    target: Option<String>,
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
    let wasm_bindgen_tree = repo_root.join("wasm-bindgen");
    if !wasm_bindgen_tree.is_dir() {
        return Err(Error::new(format!(
            "missing vendored wasm-bindgen tree at {}",
            wasm_bindgen_tree.display()
        )));
    }
    let upstream_ref = match args.upstream_ref {
        Some(upstream_ref) => upstream_ref,
        None => default_upstream_ref(&repo_root, &wasm_bindgen_tree)?,
    };

    let tmp = TempDir::new("check-wasm-bindgen-api")?;
    let upstream_repo = if has_nested_git_repo(&wasm_bindgen_tree) {
        wasm_bindgen_tree.clone()
    } else {
        let upstream_repo = tmp.path().join("upstream-wasm-bindgen-repo");
        clone_upstream_repo(&upstream_repo)?;
        upstream_repo
    };
    ensure_git_ref(&upstream_repo, &upstream_ref)?;

    let baseline_root = tmp.path().join("upstream-wasm-bindgen");
    fs::create_dir_all(&baseline_root)?;
    extract_git_archive(&upstream_repo, &upstream_ref, &baseline_root)?;
    hide_upstream_convert_module(&baseline_root)?;
    rename_baseline_package(&baseline_root)?;

    println!(
        "Checking wry-bindgen public API against upstream wasm-bindgen with cargo-semver-checks"
    );
    println!("baseline upstream ref: {upstream_ref}");
    println!("baseline root: {}", baseline_root.display());
    println!("current manifest: {CURRENT_MANIFEST}");
    println!(
        "baseline adjustments: #[doc(hidden)] on upstream convert APIs; package renamed to `{BASELINE_PACKAGE_NAME}` to match the crate under test"
    );
    println!(
        "report adjustment: wasm-ABI-trait derive differences are ignored (convert/describe codegen traits)"
    );
    if let Some(target) = args.target.as_deref() {
        println!("target: {target}");
    }
    println!();

    run_semver_checks(&repo_root, &baseline_root, args.target.as_deref())
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
            "--target" => {
                args.target = Some(
                    raw.next()
                        .ok_or_else(|| Error::new("--target requires a value"))?,
                );
            }
            _ => {
                if let Some(target) = arg.strip_prefix("--target=") {
                    args.target = Some(target.to_string());
                } else if args.upstream_ref.is_none() {
                    args.upstream_ref = Some(arg);
                } else {
                    return Err(Error::new(format!("unexpected argument `{arg}`")));
                }
            }
        }
    }

    Ok(args)
}

fn print_usage() {
    println!(
        "\
Usage:
  check-wasm-bindgen-api [wasm-bindgen-ref] [--target TARGET]

Compares the local wry-bindgen crate (which defines the desktop public API that
the `wasm-bindgen` shim re-exports) against a clean upstream wasm-bindgen tree
with `cargo semver-checks --baseline-root`. The shim itself cannot be diffed:
cargo-semver-checks (rustdoc JSON) cannot see items through the shim's cross-crate
re-exports, so the baseline package is renamed to `wry-bindgen` to line the paths up.

If wasm-bindgen-ref is omitted, patches/wasm-bindgen/BASE is used when present.
Otherwise the clean upstream tag matching wasm-bindgen/Cargo.toml is used."
    );
}

fn default_upstream_ref(repo_root: &Path, wasm_bindgen_tree: &Path) -> Result<String> {
    let patch_base = repo_root.join("patches/wasm-bindgen/BASE");
    if patch_base.is_file() {
        let text = fs::read_to_string(patch_base)?;
        if let Some(line) = text
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            return Ok(line.to_string());
        }
    }
    read_wasm_bindgen_version(&wasm_bindgen_tree.join("Cargo.toml"))
}

fn read_wasm_bindgen_version(manifest: &Path) -> Result<String> {
    let text = fs::read_to_string(manifest)?;
    let package = find_section(&text, "package").ok_or_else(|| {
        Error::new(format!(
            "{} is missing a [package] section",
            manifest.display()
        ))
    })?;
    let name = read_field(package, "name").ok_or_else(|| {
        Error::new(format!(
            "{} is missing package field `name`",
            manifest.display()
        ))
    })?;
    if name != "wasm-bindgen" && name != "not-wasm-bindgen" {
        return Err(Error::new(format!(
            "{} package name is `{name}`, expected `wasm-bindgen` or `not-wasm-bindgen`",
            manifest.display()
        )));
    }

    let version = read_field(package, "version").ok_or_else(|| {
        Error::new(format!(
            "{} is missing package field `version`",
            manifest.display()
        ))
    })?;
    if !is_stable_semver(version) {
        return Err(Error::new(format!(
            "{} package version `{version}` is not a stable semver release",
            manifest.display()
        )));
    }
    Ok(version.to_string())
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

fn read_field<'a>(section: &'a str, field: &str) -> Option<&'a str> {
    for line in section.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(field) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
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

fn is_stable_semver(version: &str) -> bool {
    let mut parts = version.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(major), Some(minor), Some(patch), None)
            if major.parse::<u64>().is_ok()
                && minor.parse::<u64>().is_ok()
                && patch.parse::<u64>().is_ok()
    )
}

fn repo_root() -> Result<PathBuf> {
    if let Some(root) = repo_root_from_source_env()? {
        return Ok(root);
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

fn repo_root_from_source_env() -> Result<Option<PathBuf>> {
    let Ok(source_path) = env::var("CHECK_WASM_BINDGEN_API_SOURCE") else {
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

fn has_nested_git_repo(repo: &Path) -> bool {
    repo.join(".git").exists()
}

fn clone_upstream_repo(output_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("clone")
        .arg("--no-checkout")
        .arg(UPSTREAM_REMOTE)
        .arg(output_dir)
        .status()
        .map_err(|error| Error::new(format!("failed to run git clone: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "failed to clone wasm-bindgen upstream repository from {UPSTREAM_REMOTE}"
        )))
    }
}

fn ensure_git_ref(repo: &Path, git_ref: &str) -> Result<()> {
    if git_ref_exists(repo, git_ref)? {
        return Ok(());
    }

    if is_stable_semver(git_ref) {
        let tag_ref = format!("refs/tags/{git_ref}:refs/tags/{git_ref}");
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["fetch", "--tags", UPSTREAM_REMOTE])
            .arg(&tag_ref)
            .status()
            .map_err(|error| Error::new(format!("failed to run git fetch: {error}")))?;
        if !status.success() {
            return Err(Error::new(format!(
                "failed to fetch wasm-bindgen upstream tag `{git_ref}`"
            )));
        }
    }

    if git_ref_exists(repo, git_ref)? {
        Ok(())
    } else {
        Err(Error::new(format!(
            "could not resolve wasm-bindgen ref `{git_ref}`"
        )))
    }
}

fn git_ref_exists(repo: &Path, git_ref: &str) -> Result<bool> {
    let tree_ref = format!("{git_ref}^{{tree}}");
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(&tree_ref)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| Error::new(format!("failed to run git rev-parse: {error}")))?;
    Ok(status.success())
}

fn extract_git_archive(repo: &Path, git_ref: &str, output_dir: &Path) -> Result<()> {
    let archive_path = output_dir.with_extension("tar");
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("archive")
        .arg("--format=tar")
        .arg(format!("--output={}", archive_path.display()))
        .arg(git_ref)
        .status()
        .map_err(|error| Error::new(format!("failed to run git archive: {error}")))?;
    if !status.success() {
        return Err(Error::new(format!(
            "git archive failed for wasm-bindgen ref `{git_ref}`"
        )));
    }

    let status = Command::new("tar")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(output_dir)
        .status()
        .map_err(|error| Error::new(format!("failed to run tar: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "failed to extract {}",
            archive_path.display()
        )))
    }
}

fn hide_upstream_convert_module(baseline_root: &Path) -> Result<()> {
    let lib_rs = baseline_root.join("src/lib.rs");
    let text = fs::read_to_string(&lib_rs)?;
    let updated = hide_convert_module_text(&text).ok_or_else(|| {
        Error::new(format!(
            "could not find `pub mod convert;` in {}",
            lib_rs.display()
        ))
    })?;

    if updated != text {
        fs::write(&lib_rs, updated)?;
    }

    Ok(())
}

fn rename_baseline_package(baseline_root: &Path) -> Result<()> {
    let manifest = baseline_root.join("Cargo.toml");
    let text = fs::read_to_string(&manifest)?;
    let updated = rename_package_text(&text, BASELINE_PACKAGE_NAME).ok_or_else(|| {
        Error::new(format!(
            "could not find `name = \"wasm-bindgen\"` package declaration in {}",
            manifest.display()
        ))
    })?;

    if updated != text {
        fs::write(&manifest, updated)?;
    }

    Ok(())
}

/// Rewrite the baseline's `[package]` name so it matches the crate under test.
/// cargo-semver-checks pairs the baseline and current crates by package name, so the
/// upstream `wasm-bindgen` baseline must be renamed to `wry-bindgen` for the diff to run.
fn rename_package_text(text: &str, new_name: &str) -> Option<String> {
    const OLD: &str = "name = \"wasm-bindgen\"";
    if !text.contains(OLD) {
        return None;
    }
    Some(text.replacen(OLD, &format!("name = \"{new_name}\""), 1))
}

fn hide_convert_module_text(text: &str) -> Option<String> {
    let replacements = [
        ("pub mod convert;", "#[doc(hidden)]\npub mod convert;"),
        (
            "    pub use crate::convert::Upcast; // provides upcast() and upcast_ref()",
            "    #[doc(hidden)]\n    pub use crate::convert::Upcast; // provides upcast() and upcast_ref()",
        ),
        (
            "pub use crate::convert::{IntoJsGeneric, JsGeneric};",
            "#[doc(hidden)]\npub use crate::convert::{IntoJsGeneric, JsGeneric};",
        ),
    ];

    let mut updated = text.to_string();
    for (plain, hidden) in replacements {
        if updated.contains(hidden) {
            continue;
        }

        if !updated.contains(plain) {
            return None;
        }

        updated = updated.replacen(plain, hidden, 1);
    }

    Some(updated)
}

fn run_semver_checks(repo_root: &Path, baseline_root: &Path, target: Option<&str>) -> Result<()> {
    let manifest = repo_root.join(CURRENT_MANIFEST);
    let mut command = Command::new("cargo");
    command
        .arg("semver-checks")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--package")
        .arg(PACKAGE)
        .arg("--baseline-root")
        .arg(baseline_root)
        .arg("--all-features")
        .arg("--release-type")
        .arg("patch")
        .arg("--color")
        .arg("never");

    if let Some(target) = target {
        command.arg("--target").arg(target);
    }

    // Capture findings (stdout) and progress (stderr) so we can filter out the
    // intentional wasm-ABI-trait derive differences before reporting / deciding.
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| Error::new(format!("failed to run cargo semver-checks: {error}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Echo progress (stderr) but drop the pre-filter verdict lines, which would be
    // misleading once the ignored ABI-trait derives are removed below.
    for line in stderr.lines() {
        if line.contains("Summary semver requires")
            || (line.contains("checks:") && line.contains("fail"))
        {
            continue;
        }
        eprintln!("{line}");
    }

    let (filtered, suppressed, remaining_categories) = filter_ignored_failures(&stdout);
    print!("{filtered}");
    if !filtered.is_empty() && !filtered.ends_with('\n') {
        println!();
    }
    if suppressed > 0 {
        println!(
            "note: ignored {suppressed} known intentional difference(s): wasm-ABI-trait derives \
             (convert/describe) and JsThreadLocal's private fields."
        );
    }

    if output.status.success() {
        return Ok(());
    }
    if remaining_categories == 0 {
        println!(
            "public API diff: only known intentional differences remained; treating as compatible."
        );
        Ok(())
    } else {
        Err(Error::new(
            "public API differences found (see report above)",
        ))
    }
}

/// Remove the known intentional findings (see [`IGNORED_DERIVE_TRAITS`] and
/// [`IGNORED_CONSTRUCTIBLE_TYPES`]) from a cargo-semver-checks report. If a lint's block
/// is left with no findings, the whole block is dropped. Returns the filtered report, the
/// number of suppressed findings, and the number of failure categories that remain.
fn filter_ignored_failures(stdout: &str) -> (String, usize, usize) {
    let lines: Vec<&str> = stdout.lines().collect();
    let block_starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("--- failure "))
        .map(|(index, _)| index)
        .collect();

    if block_starts.is_empty() {
        return (stdout.to_string(), 0, 0);
    }

    let mut out = String::new();
    for line in &lines[..block_starts[0]] {
        out.push_str(line);
        out.push('\n');
    }

    let mut suppressed = 0;
    let mut remaining_categories = 0;

    for (position, &start) in block_starts.iter().enumerate() {
        let end = block_starts
            .get(position + 1)
            .copied()
            .unwrap_or(lines.len());
        let block = &lines[start..end];

        let Some(is_ignored) = ignored_finding_predicate(lint_name(block[0])) else {
            for line in block {
                out.push_str(line);
                out.push('\n');
            }
            remaining_categories += 1;
            continue;
        };

        let mut kept: Vec<&str> = Vec::new();
        let mut kept_items = 0;
        let mut in_failed_in = false;
        for &line in block {
            if line.starts_with("Failed in:") {
                in_failed_in = true;
                kept.push(line);
                continue;
            }
            let is_item = in_failed_in && line.starts_with("  ") && !line.trim().is_empty();
            if is_item {
                if is_ignored(line) {
                    suppressed += 1;
                } else {
                    kept_items += 1;
                    kept.push(line);
                }
                continue;
            }
            kept.push(line);
        }

        if kept_items == 0 {
            continue;
        }
        for line in kept {
            out.push_str(line);
            out.push('\n');
        }
        remaining_categories += 1;
    }

    (out, suppressed, remaining_categories)
}

/// The lint name from a `--- failure <lint>: ... ---` header.
fn lint_name(header: &str) -> &str {
    header
        .strip_prefix("--- failure ")
        .and_then(|rest| rest.split(':').next())
        .unwrap_or("")
        .trim()
}

/// The per-finding predicate for a filterable lint, if any.
fn ignored_finding_predicate(lint: &str) -> Option<fn(&str) -> bool> {
    match lint {
        "derive_trait_impl_removed" => Some(is_ignored_derive_failure),
        "constructible_struct_adds_private_field" => Some(is_ignored_constructible_field),
        _ => None,
    }
}

/// Whether a `Failed in:` detail line is a `no longer derives <trait>` finding for a
/// trait in [`IGNORED_DERIVE_TRAITS`].
fn is_ignored_derive_failure(line: &str) -> bool {
    const MARKER: &str = " no longer derives ";
    let Some(index) = line.find(MARKER) else {
        return false;
    };
    let rest = &line[index + MARKER.len()..];
    let trait_name = rest
        .split(|c: char| c == ',' || c.is_whitespace())
        .next()
        .unwrap_or("");
    IGNORED_DERIVE_TRAITS.contains(&trait_name)
}

/// Whether a `Failed in:` detail line is a `field <Type>.<field>` finding for a type in
/// [`IGNORED_CONSTRUCTIBLE_TYPES`].
fn is_ignored_constructible_field(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("field ") else {
        return false;
    };
    let type_name = rest.split('.').next().unwrap_or("");
    IGNORED_CONSTRUCTIBLE_TYPES.contains(&type_name)
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self> {
        let mut path = env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| Error::new(format!("system time is before unix epoch: {error}")))?
            .as_nanos();
        path.push(format!("{prefix}-{}-{nanos}", process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        filter_ignored_failures, hide_convert_module_text, is_ignored_constructible_field,
        is_ignored_derive_failure, rename_package_text,
    };

    #[test]
    fn detects_ignored_vs_real_derive_lines() {
        assert!(is_ignored_derive_failure(
            "  type Null no longer derives FromWasmAbi, in /p/sys.rs:109"
        ));
        assert!(is_ignored_derive_failure(
            "  type JsValue no longer derives UpcastFrom, in /p/value.rs:48"
        ));
        // A real std-derive removal stays a failure.
        assert!(!is_ignored_derive_failure(
            "  type JsError no longer derives Clone, in /p/js_error.rs:10"
        ));
        assert!(!is_ignored_derive_failure(
            "  JsValue::from_serde, previously in file /p/lib.rs:296"
        ));
    }

    #[test]
    fn detects_ignored_constructible_field() {
        assert!(is_ignored_constructible_field(
            "  field JsThreadLocal.key in /p/lazy.rs:28"
        ));
        // A different type's added private field stays a failure.
        assert!(!is_ignored_constructible_field(
            "  field SomeOther.inner in /p/x.rs:1"
        ));
    }

    #[test]
    fn drops_derive_block_when_only_abi_traits() {
        let input = "\
--- failure derive_trait_impl_removed: built-in derived trait no longer implemented ---

Failed in:
  type JsOption no longer derives FromWasmAbi, in /p/sys.rs:118
  type Null no longer derives WasmDescribe, in /p/sys.rs:109

--- failure inherent_method_missing: pub method removed or renamed ---

Failed in:
  JsValue::from_serde, previously in file /p/lib.rs:296
";
        let (out, suppressed, remaining) = filter_ignored_failures(input);
        assert_eq!(suppressed, 2);
        assert_eq!(remaining, 1);
        assert!(!out.contains("derive_trait_impl_removed"));
        assert!(out.contains("inherent_method_missing"));
        assert!(out.contains("from_serde"));
    }

    #[test]
    fn keeps_derive_block_when_real_derive_present() {
        let input = "\
--- failure derive_trait_impl_removed: built-in derived trait no longer implemented ---

Failed in:
  type JsError no longer derives Clone, in /p/js_error.rs:10
  type JsOption no longer derives IntoWasmAbi, in /p/sys.rs:118
";
        let (out, suppressed, remaining) = filter_ignored_failures(input);
        assert_eq!(suppressed, 1); // only IntoWasmAbi suppressed
        assert_eq!(remaining, 1); // block kept because `Clone` remains
        assert!(out.contains("no longer derives Clone"));
        assert!(!out.contains("IntoWasmAbi"));
    }

    #[test]
    fn drops_constructible_block_for_ignored_type() {
        let input = "\
--- failure constructible_struct_adds_private_field: struct no longer constructible ---

Failed in:
  field JsThreadLocal.key in /p/lazy.rs:28
  field JsThreadLocal.init in /p/lazy.rs:29
";
        let (out, suppressed, remaining) = filter_ignored_failures(input);
        assert_eq!(suppressed, 2);
        assert_eq!(remaining, 0);
        assert!(!out.contains("constructible_struct_adds_private_field"));
    }

    #[test]
    fn passes_through_when_no_failures() {
        let input = "Checking ...\nall good\n";
        let (out, suppressed, remaining) = filter_ignored_failures(input);
        assert_eq!(out, input);
        assert_eq!(suppressed, 0);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn renames_package_name() {
        let input = "[package]\nname = \"wasm-bindgen\"\nversion = \"0.2.122\"\n";
        let output =
            rename_package_text(input, "wry-bindgen").expect("package name should be found");
        assert_eq!(
            output,
            "[package]\nname = \"wry-bindgen\"\nversion = \"0.2.122\"\n"
        );
    }

    #[test]
    fn rename_package_only_touches_first_match() {
        // Dependency entries must not be rewritten, only the `[package]` name.
        let input =
            "[package]\nname = \"wasm-bindgen\"\n\n[dependencies]\nwasm-bindgen-macro = \"0.2\"\n";
        let output =
            rename_package_text(input, "wry-bindgen").expect("package name should be found");
        assert_eq!(
            output,
            "[package]\nname = \"wry-bindgen\"\n\n[dependencies]\nwasm-bindgen-macro = \"0.2\"\n"
        );
    }

    #[test]
    fn missing_package_name_returns_none() {
        assert!(rename_package_text("[package]\nname = \"other\"\n", "wry-bindgen").is_none());
    }

    #[test]
    fn hides_convert_module_declaration() {
        let input = "\
pub mod prelude {
    pub use crate::convert::Upcast; // provides upcast() and upcast_ref()
}
pub mod closure;
pub mod convert;
pub mod describe;
pub use crate::convert::{IntoJsGeneric, JsGeneric};
";
        let output = hide_convert_module_text(input).expect("convert module should be found");
        assert_eq!(
            output,
            "\
pub mod prelude {
    #[doc(hidden)]
    pub use crate::convert::Upcast; // provides upcast() and upcast_ref()
}
pub mod closure;
#[doc(hidden)]
pub mod convert;
pub mod describe;
#[doc(hidden)]
pub use crate::convert::{IntoJsGeneric, JsGeneric};
"
        );
    }

    #[test]
    fn hide_convert_module_is_idempotent() {
        let input = "\
pub mod prelude {
    #[doc(hidden)]
    pub use crate::convert::Upcast; // provides upcast() and upcast_ref()
}
#[doc(hidden)]
pub mod convert;
#[doc(hidden)]
pub use crate::convert::{IntoJsGeneric, JsGeneric};
";
        let output = hide_convert_module_text(input).expect("convert module should be found");
        assert_eq!(output, input);
    }

    #[test]
    fn missing_convert_module_returns_none() {
        assert!(hide_convert_module_text("pub mod closure;\n").is_none());
    }
}
