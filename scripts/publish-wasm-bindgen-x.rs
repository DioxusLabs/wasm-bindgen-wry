#!/bin/sh
//usr/bin/env rustc --edition=2024 "$0" -o "${TMPDIR:-/tmp}/publish-wasm-bindgen-x" && PUBLISH_WASM_BINDGEN_X_SOURCE="$0" "${TMPDIR:-/tmp}/publish-wasm-bindgen-x" "$@"; exit $?

// Run directly with:
//   ./scripts/publish-wasm-bindgen-x.rs [--dry-run|--publish] [--prepare-only]
//
// The script creates a staging tree, rewrites the crates.io package names for
// the wasm-bindgen-facing crates to their `-x` names, and runs `cargo publish`
// from the staged manifests. The source tree is not modified.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const MARKER_FILE: &str = ".publish-wasm-bindgen-x-staging";

const COPY_ENTRIES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "wry-bindgen",
    "wry-bindgen-macro",
    "wry-bindgen-macro-support",
    "shims",
    "wasm-bindgen/Cargo.toml",
    "wasm-bindgen/LICENSE-APACHE",
    "wasm-bindgen/LICENSE-MIT",
    "wasm-bindgen/crates/js-sys",
    "wasm-bindgen/crates/web-sys",
    "wasm-bindgen/crates/futures",
];

const RENAMED_CRATES: &[RenamedCrate] = &[
    RenamedCrate {
        manifest: "shims/wasm-bindgen/Cargo.toml",
        source_name: "wasm-bindgen",
        publish_name: "wasm-bindgen-x",
        lib_name: "wasm_bindgen",
    },
    RenamedCrate {
        manifest: "shims/wasm-bindgen-macro/Cargo.toml",
        source_name: "wasm-bindgen-macro",
        publish_name: "wasm-bindgen-macro-x",
        lib_name: "wasm_bindgen_macro",
    },
    RenamedCrate {
        manifest: "wasm-bindgen/crates/js-sys/Cargo.toml",
        source_name: "js-sys",
        publish_name: "js-sys-x",
        lib_name: "js_sys",
    },
    RenamedCrate {
        manifest: "wasm-bindgen/crates/web-sys/Cargo.toml",
        source_name: "web-sys",
        publish_name: "web-sys-x",
        lib_name: "web_sys",
    },
    RenamedCrate {
        manifest: "wasm-bindgen/crates/futures/Cargo.toml",
        source_name: "wasm-bindgen-futures",
        publish_name: "wasm-bindgen-futures-x",
        lib_name: "wasm_bindgen_futures",
    },
];

const UNRENAMED_PUBLISH_CRATES: &[PublishCrate] = &[
    PublishCrate {
        manifest: "wry-bindgen-macro-support/Cargo.toml",
        publish_name: "wry-bindgen-macro-support",
    },
    PublishCrate {
        manifest: "wry-bindgen-macro/Cargo.toml",
        publish_name: "wry-bindgen-macro",
    },
    PublishCrate {
        manifest: "wry-bindgen/Cargo.toml",
        publish_name: "wry-bindgen",
    },
];

#[derive(Clone, Copy)]
struct RenamedCrate {
    manifest: &'static str,
    source_name: &'static str,
    publish_name: &'static str,
    lib_name: &'static str,
}

#[derive(Clone, Copy)]
struct PublishCrate {
    manifest: &'static str,
    publish_name: &'static str,
}

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
    publish: bool,
    publish_set: bool,
    prepare_only: bool,
    staging_dir: Option<PathBuf>,
    packages: Vec<String>,
    registry: Option<String>,
    no_verify: bool,
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
    let staging_dir = args
        .staging_dir
        .clone()
        .unwrap_or_else(|| repo_root.join("target/publish-wasm-bindgen-x"));

    prepare_staging(&repo_root, &staging_dir)?;

    let publish_crates = selected_publish_crates(&args)?;
    println!("prepared publish staging tree: {}", staging_dir.display());
    println!("renamed crates.io packages:");
    for krate in RENAMED_CRATES {
        let version = read_package_version(&staging_dir.join(krate.manifest))?;
        println!(
            "  {} {} -> {}",
            krate.source_name, version, krate.publish_name
        );
    }

    if args.prepare_only {
        println!("prepare-only mode: no cargo publish commands were run");
        return Ok(());
    }

    let mode = if args.publish { "publish" } else { "dry-run" };
    println!("running cargo publish in {mode} mode:");
    for krate in publish_crates {
        run_cargo_publish(&staging_dir, krate, &args)?;
    }

    Ok(())
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
            "--dry-run" => {
                if args.publish_set && args.publish {
                    return Err(Error::new(
                        "--dry-run and --publish cannot be used together",
                    ));
                }
                args.publish = false;
                args.publish_set = true;
            }
            "--publish" | "--wet-run" => {
                if args.publish_set && !args.publish {
                    return Err(Error::new(
                        "--dry-run and --publish cannot be used together",
                    ));
                }
                args.publish = true;
                args.publish_set = true;
            }
            "--prepare-only" => args.prepare_only = true,
            "--staging-dir" => {
                args.staging_dir =
                    Some(PathBuf::from(raw.next().ok_or_else(|| {
                        Error::new("--staging-dir requires a value")
                    })?));
            }
            "--package" | "-p" => {
                args.packages.push(
                    raw.next()
                        .ok_or_else(|| Error::new("--package requires a value"))?,
                );
            }
            "--registry" => {
                args.registry = Some(
                    raw.next()
                        .ok_or_else(|| Error::new("--registry requires a value"))?,
                );
            }
            "--no-verify" => args.no_verify = true,
            _ => {
                if let Some(path) = arg.strip_prefix("--staging-dir=") {
                    args.staging_dir = Some(PathBuf::from(path));
                } else if let Some(package) = arg.strip_prefix("--package=") {
                    args.packages.push(package.to_string());
                } else if let Some(registry) = arg.strip_prefix("--registry=") {
                    args.registry = Some(registry.to_string());
                } else {
                    return Err(Error::new(format!("unknown argument `{arg}`")));
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
  publish-wasm-bindgen-x [--dry-run|--publish] [options]

Options:
  --dry-run             Run cargo publish --dry-run after staging. This is the default.
  --publish, --wet-run  Run real cargo publish after staging.
  --prepare-only        Only create and rewrite the staging tree.
  --staging-dir PATH    Staging directory. Defaults to target/publish-wasm-bindgen-x.
  -p, --package NAME    Publish only one package. May be repeated.
  --registry NAME       Pass --registry NAME to cargo publish.
  --no-verify           Pass --no-verify to cargo publish.

The script rewrites package names only in the staging tree:
  wasm-bindgen -> wasm-bindgen-x
  wasm-bindgen-macro -> wasm-bindgen-macro-x
  js-sys -> js-sys-x
  web-sys -> web-sys-x
  wasm-bindgen-futures -> wasm-bindgen-futures-x
"
    );
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
    let Ok(source_path) = env::var("PUBLISH_WASM_BINDGEN_X_SOURCE") else {
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

fn selected_publish_crates(args: &Args) -> Result<Vec<PublishCrate>> {
    let all = publish_crates();
    if args.packages.is_empty() {
        return Ok(all);
    }

    let requested: BTreeSet<_> = args.packages.iter().map(String::as_str).collect();
    let selected: Vec<_> = all
        .into_iter()
        .filter(|krate| {
            requested
                .iter()
                .any(|package| package_request_matches(krate, package))
        })
        .collect();

    let known = publish_crates();
    let missing: Vec<_> = requested
        .iter()
        .filter(|package| {
            !known
                .iter()
                .any(|krate| package_request_matches(krate, package))
        })
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(Error::new(format!(
            "unknown package(s): {}; known packages: {}",
            missing.join(", "),
            publish_crates()
                .iter()
                .map(|krate| krate.publish_name)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    Ok(selected)
}

fn package_request_matches(krate: &PublishCrate, request: &str) -> bool {
    if krate.publish_name == request {
        return true;
    }

    RENAMED_CRATES.iter().any(|renamed| {
        renamed.manifest == krate.manifest
            && (renamed.source_name == request || renamed.publish_name == request)
    })
}

fn publish_crates() -> Vec<PublishCrate> {
    let mut crates = Vec::new();
    crates.extend_from_slice(UNRENAMED_PUBLISH_CRATES);
    crates.extend(RENAMED_CRATES.iter().map(|krate| PublishCrate {
        manifest: krate.manifest,
        publish_name: krate.publish_name,
    }));
    crates
}

fn prepare_staging(repo_root: &Path, staging_dir: &Path) -> Result<()> {
    reset_staging_dir(repo_root, staging_dir)?;

    for relative in COPY_ENTRIES {
        copy_entry(&repo_root.join(relative), &staging_dir.join(relative))?;
    }

    rewrite_staging_manifests(staging_dir)?;
    verify_staging(staging_dir)?;
    Ok(())
}

fn reset_staging_dir(repo_root: &Path, staging_dir: &Path) -> Result<()> {
    if staging_dir == repo_root {
        return Err(Error::new(
            "refusing to use the repo root as the staging directory",
        ));
    }
    if staging_dir.exists() {
        if !staging_dir.join(MARKER_FILE).is_file() {
            return Err(Error::new(format!(
                "refusing to remove {}; it is not marked as a publish staging tree",
                staging_dir.display()
            )));
        }
        fs::remove_dir_all(staging_dir)?;
    }

    fs::create_dir_all(staging_dir)?;
    fs::write(
        staging_dir.join(MARKER_FILE),
        "generated by scripts/publish-wasm-bindgen-x.rs\n",
    )?;
    Ok(())
}

fn copy_entry(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::metadata(source).map_err(|error| {
        Error::new(format!(
            "failed to read {} while preparing publish staging tree: {error}",
            source.display()
        ))
    })?;

    if metadata.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name();
            if name == "target" || name == ".git" || name == ".DS_Store" {
                continue;
            }
            copy_entry(&entry.path(), &destination.join(name))?;
        }
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}

fn rewrite_staging_manifests(staging_dir: &Path) -> Result<()> {
    let versions = package_versions(staging_dir)?;

    keep_only_workspace_sections(&staging_dir.join("wasm-bindgen/Cargo.toml"))?;

    for krate in RENAMED_CRATES {
        let manifest = staging_dir.join(krate.manifest);
        rename_package(&manifest, krate.source_name, krate.publish_name)?;
        ensure_lib_name(&manifest, krate.lib_name)?;
    }

    for krate in publish_crates() {
        rewrite_dependency_packages(&staging_dir.join(krate.manifest), &versions, true)?;
    }

    rewrite_root_workspace_members(&staging_dir.join("Cargo.toml"))?;
    rewrite_dependency_packages(&staging_dir.join("Cargo.toml"), &versions, false)?;
    Ok(())
}

fn keep_only_workspace_sections(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let lines = Lines::from(&text);
    let mut output = String::new();
    let mut kept_any = false;
    let mut index = 0;

    while index < lines.len() {
        let Some(name) = section_name(lines.body(index)) else {
            index += 1;
            continue;
        };

        let start = index;
        index += 1;
        while index < lines.len() && section_name(lines.body(index)).is_none() {
            index += 1;
        }

        if name == "workspace" || name.starts_with("workspace.") {
            for line in &lines.lines[start..index] {
                output.push_str(&line.body);
                output.push_str(line.ending);
            }
            if !output.ends_with("\n\n") {
                output.push('\n');
            }
            kept_any = true;
        }
    }

    if !kept_any {
        return Err(Error::new(format!(
            "{} has no [workspace] sections to preserve",
            path.display()
        )));
    }

    fs::write(path, output)?;
    Ok(())
}

fn rewrite_root_workspace_members(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = Lines::from(&text);
    let (start, end) = lines.find_section_bounds("workspace").ok_or_else(|| {
        Error::new(format!(
            "{} is missing a [workspace] section",
            path.display()
        ))
    })?;

    for index in start..end {
        let trimmed = lines.body(index).trim_start();
        if !trimmed.starts_with("members") || !trimmed.contains('[') {
            continue;
        }

        let mut close = index;
        while close < end && lines.body(close).trim() != "]" {
            close += 1;
        }
        if close == end {
            return Err(Error::new(format!(
                "{} has an unterminated workspace members array",
                path.display()
            )));
        }

        lines.replace_range(
            index,
            close + 1,
            &[
                "members = [",
                "    \"wry-bindgen\",",
                "    \"wry-bindgen-macro\",",
                "    \"wry-bindgen-macro-support\",",
                "    \"shims/wasm-bindgen\",",
                "    \"shims/wasm-bindgen-macro\",",
                "]",
            ],
        );
        fs::write(path, lines.into_string())?;
        return Ok(());
    }

    Err(Error::new(format!(
        "{} is missing workspace field `members`",
        path.display()
    )))
}

fn package_versions(staging_dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut versions = BTreeMap::new();

    for krate in RENAMED_CRATES {
        let version = read_package_version(&staging_dir.join(krate.manifest))?;
        versions.insert(krate.source_name.to_string(), version.clone());
        versions.insert(krate.publish_name.to_string(), version);
    }
    for krate in UNRENAMED_PUBLISH_CRATES {
        let version = read_package_version(&staging_dir.join(krate.manifest))?;
        versions.insert(krate.publish_name.to_string(), version);
    }

    Ok(versions)
}

fn verify_staging(staging_dir: &Path) -> Result<()> {
    for krate in RENAMED_CRATES {
        let manifest = staging_dir.join(krate.manifest);
        let name = read_package_name(&manifest)?;
        if name != krate.publish_name {
            return Err(Error::new(format!(
                "{} package name is `{name}`, expected `{}`",
                manifest.display(),
                krate.publish_name
            )));
        }
    }

    for krate in publish_crates() {
        let manifest = staging_dir.join(krate.manifest);
        let name = read_package_name(&manifest)?;
        if name != krate.publish_name {
            return Err(Error::new(format!(
                "{} package name is `{name}`, expected `{}`",
                manifest.display(),
                krate.publish_name
            )));
        }
    }

    Ok(())
}

fn rename_package(path: &Path, source_name: &str, publish_name: &str) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = Lines::from(&text);
    let (start, end) = lines
        .find_section_bounds("package")
        .ok_or_else(|| Error::new(format!("{} is missing a [package] section", path.display())))?;

    for index in start..end {
        let Some(current) = lines.field_value(index, "name") else {
            continue;
        };
        if current != source_name {
            return Err(Error::new(format!(
                "{} package name is `{current}`, expected `{source_name}`",
                path.display()
            )));
        }
        lines.replace_field(index, "name", publish_name);
        fs::write(path, lines.into_string())?;
        return Ok(());
    }

    Err(Error::new(format!(
        "{} is missing package field `name`",
        path.display()
    )))
}

fn ensure_lib_name(path: &Path, lib_name: &str) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = Lines::from(&text);

    if let Some((start, end)) = lines.find_section_bounds("lib") {
        for index in start..end {
            if lines.replace_field(index, "name", lib_name) {
                fs::write(path, lines.into_string())?;
                return Ok(());
            }
        }
        lines.insert(start, format!("name = \"{lib_name}\""));
        fs::write(path, lines.into_string())?;
        return Ok(());
    }

    let (_, package_end) = lines
        .find_section_bounds("package")
        .ok_or_else(|| Error::new(format!("{} is missing a [package] section", path.display())))?;
    lines.insert(package_end, String::new());
    lines.insert(package_end + 1, "[lib]".to_string());
    lines.insert(package_end + 2, format!("name = \"{lib_name}\""));
    fs::write(path, lines.into_string())?;
    Ok(())
}

fn rewrite_dependency_packages(
    path: &Path,
    versions: &BTreeMap<String, String>,
    add_versions: bool,
) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = Lines::from(&text);
    let mut changed = false;

    for index in 0..lines.len() {
        let line = lines.body(index).to_string();
        let Some((prefix, key, table_body, suffix)) = split_inline_table(&line) else {
            continue;
        };
        let dependency_name = inline_table_value(table_body, "package").unwrap_or(key);
        let Some(publish_name) = renamed_package_name(dependency_name) else {
            continue;
        };

        let mut updated_table = upsert_inline_table_value(table_body, "package", publish_name);
        if add_versions {
            let version = versions.get(publish_name).ok_or_else(|| {
                Error::new(format!("missing package version for `{publish_name}`"))
            })?;
            updated_table =
                upsert_inline_table_value(&updated_table, "version", &format!("={version}"));
        }
        lines.set_body(index, format!("{prefix}{updated_table}{suffix}"));
        changed = true;
    }

    if changed {
        fs::write(path, lines.into_string())?;
    }
    Ok(())
}

fn renamed_package_name(name: &str) -> Option<&'static str> {
    RENAMED_CRATES
        .iter()
        .find_map(|krate| (krate.source_name == name).then_some(krate.publish_name))
}

fn read_package_name(path: &Path) -> Result<String> {
    read_package_field(path, "name")
}

fn read_package_version(path: &Path) -> Result<String> {
    read_package_field(path, "version")
}

fn read_package_field(path: &Path, field: &str) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let package = find_section(&text, "package")
        .ok_or_else(|| Error::new(format!("{} is missing a [package] section", path.display())))?;
    read_field(package, field)
        .map(str::to_string)
        .ok_or_else(|| {
            Error::new(format!(
                "{} is missing package field `{field}`",
                path.display()
            ))
        })
}

fn run_cargo_publish(staging_dir: &Path, krate: PublishCrate, args: &Args) -> Result<()> {
    let manifest = staging_dir.join(krate.manifest);
    let crate_dir = manifest
        .parent()
        .ok_or_else(|| Error::new(format!("{} has no parent directory", manifest.display())))?;

    let version = read_package_version(&manifest)?;
    let mut command = Command::new("cargo");
    command.arg("publish");
    if !args.publish {
        command.arg("--dry-run");
    }
    if args.no_verify {
        command.arg("--no-verify");
    }
    if let Some(registry) = &args.registry {
        command.args(["--registry", registry]);
    }
    command.current_dir(crate_dir);

    println!("  {} {}", krate.publish_name, version);
    let status = command.status().map_err(|error| {
        Error::new(format!(
            "failed to run cargo publish for {}: {error}",
            krate.publish_name
        ))
    })?;
    if !status.success() {
        return Err(Error::new(format!(
            "cargo publish failed for {} with status {status}",
            krate.publish_name
        )));
    }

    Ok(())
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

fn upsert_inline_table_value(table: &str, key: &str, value: &str) -> String {
    if inline_table_value(table, key).is_some() {
        return replace_inline_table_value(table, key, value);
    }

    let trimmed_end = table.trim_end();
    let trailing = &table[trimmed_end.len()..];
    if trimmed_end.trim().is_empty() {
        return format!(" {key} = \"{value}\"{trailing}");
    }

    format!("{trimmed_end}, {key} = \"{value}\"{trailing}")
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

    fn insert(&mut self, index: usize, body: String) {
        self.lines.insert(index, Line { body, ending: "\n" });
    }

    fn replace_range(&mut self, start: usize, end: usize, bodies: &[&str]) {
        self.lines.splice(
            start..end,
            bodies.iter().map(|body| Line {
                body: (*body).to_string(),
                ending: "\n",
            }),
        );
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

    fn field_value<'a>(&'a self, index: usize, field: &str) -> Option<&'a str> {
        let trimmed = self.lines[index].body.trim_start();
        let rest = strip_toml_field_prefix(trimmed, field)?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('"')?;
        let end = rest.find('"')?;
        Some(&rest[..end])
    }

    fn replace_field(&mut self, index: usize, field: &str, value: &str) -> bool {
        let trimmed = self.lines[index].body.trim_start();
        let Some(rest) = strip_toml_field_prefix(trimmed, field) else {
            return false;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('"') {
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
        self.lines[index].body = updated;
        true
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
    fn upsert_adds_missing_inline_table_values() {
        let table = r#" path = "../js-sys", default-features = false "#;
        let table = upsert_inline_table_value(table, "package", "js-sys-x");
        let table = upsert_inline_table_value(&table, "version", "=0.3.99");

        assert!(table.contains(r#"package = "js-sys-x""#));
        assert!(table.contains(r#"version = "=0.3.99""#));
        assert!(table.contains("default-features = false"));
    }

    #[test]
    fn upsert_replaces_existing_inline_table_values() {
        let table = r#" path = "shim", package = "wasm-bindgen", version = "=0.2.122" "#;
        let table = upsert_inline_table_value(table, "package", "wasm-bindgen-x");

        assert!(table.contains(r#"package = "wasm-bindgen-x""#));
        assert!(!table.contains(r#"package = "wasm-bindgen""#));
    }

    #[test]
    fn ensure_lib_name_inserts_name_in_existing_lib_section() {
        let input = "[package]\nname = \"js-sys-x\"\n\n[lib]\ntest = false\n";
        let mut lines = Lines::from(input);
        let (start, _) = lines.find_section_bounds("lib").unwrap();
        lines.insert(start, "name = \"js_sys\"".to_string());

        let output = lines.into_string();
        assert!(output.contains("[lib]\nname = \"js_sys\"\ntest = false"));
    }

    #[test]
    fn renamed_package_map_contains_expected_crates() {
        assert_eq!(renamed_package_name("wasm-bindgen"), Some("wasm-bindgen-x"));
        assert_eq!(renamed_package_name("js-sys"), Some("js-sys-x"));
        assert_eq!(renamed_package_name("serde"), None);
    }
}
