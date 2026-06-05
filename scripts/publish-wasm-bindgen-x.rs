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
const WEB_SYS_MANIFEST: &str = "vendored/wasm-bindgen/crates/web-sys/Cargo.toml";
const PUBLISHED_WEB_SYS_PACKAGE: &str = "web-sys-x";
const CRATES_IO_FEATURE_LIMIT: usize = 300;

const COPY_ENTRIES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "packages/wry-bindgen",
    "packages/wry-bindgen-core",
    "packages/wry-bindgen-runtime",
    "packages/wry-bindgen-macro",
    "packages/wry-bindgen-macro-support",
    "packages/wasm-bindgen",
    "packages/wasm-bindgen-macro",
    "vendored/wasm-bindgen/Cargo.toml",
    "vendored/wasm-bindgen/LICENSE-APACHE",
    "vendored/wasm-bindgen/LICENSE-MIT",
    "vendored/wasm-bindgen/crates/shared",
    "vendored/wasm-bindgen/crates/macro-support",
    "vendored/wasm-bindgen/crates/js-sys",
    "vendored/wasm-bindgen/crates/web-sys",
    "vendored/wasm-bindgen/crates/futures",
];

const RENAMED_CRATES: &[RenamedCrate] = &[
    RenamedCrate {
        manifest: "packages/wasm-bindgen/Cargo.toml",
        source_name: "wasm-bindgen",
        publish_name: "wasm-bindgen-x",
        lib_name: "wasm_bindgen",
    },
    RenamedCrate {
        manifest: "packages/wasm-bindgen-macro/Cargo.toml",
        source_name: "wasm-bindgen-macro",
        publish_name: "wasm-bindgen-macro-x",
        lib_name: "wasm_bindgen_macro",
    },
    RenamedCrate {
        manifest: "vendored/wasm-bindgen/crates/macro-support/Cargo.toml",
        source_name: "wasm-bindgen-macro-support",
        publish_name: "wasm-bindgen-macro-support-x",
        lib_name: "wasm_bindgen_macro_support",
    },
    RenamedCrate {
        manifest: "vendored/wasm-bindgen/crates/js-sys/Cargo.toml",
        source_name: "js-sys",
        publish_name: "js-sys-x",
        lib_name: "js_sys",
    },
    RenamedCrate {
        manifest: "vendored/wasm-bindgen/crates/web-sys/Cargo.toml",
        source_name: "web-sys",
        publish_name: "web-sys-x",
        lib_name: "web_sys",
    },
    RenamedCrate {
        manifest: "vendored/wasm-bindgen/crates/futures/Cargo.toml",
        source_name: "wasm-bindgen-futures",
        publish_name: "wasm-bindgen-futures-x",
        lib_name: "wasm_bindgen_futures",
    },
];

const UNRENAMED_PUBLISH_CRATES: &[PublishCrate] = &[
    PublishCrate {
        manifest: "packages/wry-bindgen-macro-support/Cargo.toml",
        publish_name: "wry-bindgen-macro-support",
    },
    PublishCrate {
        manifest: "packages/wry-bindgen-runtime/Cargo.toml",
        publish_name: "wry-bindgen-runtime",
    },
    PublishCrate {
        manifest: "packages/wry-bindgen-core/Cargo.toml",
        publish_name: "wry-bindgen-core",
    },
    PublishCrate {
        manifest: "packages/wry-bindgen-macro/Cargo.toml",
        publish_name: "wry-bindgen-macro",
    },
    PublishCrate {
        manifest: "packages/wry-bindgen/Cargo.toml",
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

    if args.publish {
        println!("running cargo publish crate-by-crate:");
        for krate in publish_crates {
            run_cargo_publish(&staging_dir, krate, &args)?;
        }
    } else {
        println!("running cargo publish --dry-run from the staged workspace:");
        run_workspace_dry_run(&staging_dir, &publish_crates, &args)?;
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
            "--no-verify" => args.no_verify = true,
            _ => {
                if let Some(path) = arg.strip_prefix("--staging-dir=") {
                    args.staging_dir = Some(PathBuf::from(path));
                } else if let Some(package) = arg.strip_prefix("--package=") {
                    args.packages.push(package.to_string());
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
  --dry-run             Run workspace cargo publish --dry-run after staging. This is the default.
  --publish, --wet-run  Run real cargo publish after staging.
  --prepare-only        Only create and rewrite the staging tree.
  --staging-dir PATH    Staging directory. Defaults to target/publish-wasm-bindgen-x.
  -p, --package NAME    Publish only one package. May be repeated.
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
            && ancestor.join("packages/wry-bindgen").is_dir()
            && ancestor.join("packages/wasm-bindgen").is_dir()
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
    vec![
        PublishCrate {
            manifest: "vendored/wasm-bindgen/crates/macro-support/Cargo.toml",
            publish_name: "wasm-bindgen-macro-support-x",
        },
        PublishCrate {
            manifest: "packages/wry-bindgen-macro-support/Cargo.toml",
            publish_name: "wry-bindgen-macro-support",
        },
        PublishCrate {
            manifest: "packages/wry-bindgen-runtime/Cargo.toml",
            publish_name: "wry-bindgen-runtime",
        },
        PublishCrate {
            manifest: "packages/wry-bindgen-core/Cargo.toml",
            publish_name: "wry-bindgen-core",
        },
        PublishCrate {
            manifest: "packages/wry-bindgen-macro/Cargo.toml",
            publish_name: "wry-bindgen-macro",
        },
        PublishCrate {
            manifest: "packages/wry-bindgen/Cargo.toml",
            publish_name: "wry-bindgen",
        },
        PublishCrate {
            manifest: "packages/wasm-bindgen-macro/Cargo.toml",
            publish_name: "wasm-bindgen-macro-x",
        },
        PublishCrate {
            manifest: "packages/wasm-bindgen/Cargo.toml",
            publish_name: "wasm-bindgen-x",
        },
        PublishCrate {
            manifest: "vendored/wasm-bindgen/crates/js-sys/Cargo.toml",
            publish_name: "js-sys-x",
        },
        PublishCrate {
            manifest: "vendored/wasm-bindgen/crates/web-sys/Cargo.toml",
            publish_name: "web-sys-x",
        },
        PublishCrate {
            manifest: "vendored/wasm-bindgen/crates/futures/Cargo.toml",
            publish_name: "wasm-bindgen-futures-x",
        },
    ]
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

    merge_vendored_workspace_lints(staging_dir)?;
    remove_local_vendored_workspace_roots(staging_dir)?;

    for krate in RENAMED_CRATES {
        let manifest = staging_dir.join(krate.manifest);
        rename_package(&manifest, krate.source_name, krate.publish_name)?;
        ensure_lib_name(&manifest, krate.lib_name)?;
    }

    for krate in publish_crates() {
        rewrite_dependency_packages(&staging_dir.join(krate.manifest), &versions, true)?;
    }

    let root_manifest = staging_dir.join("Cargo.toml");
    rewrite_root_workspace_members(&root_manifest)?;
    rewrite_dependency_packages(&root_manifest, &versions, false)?;
    ensure_patch_crates_io_entry(
        &root_manifest,
        "web-sys",
        "vendored/wasm-bindgen/crates/web-sys",
        "web-sys-x",
    )?;
    trim_web_sys_features_to_published(staging_dir)?;
    Ok(())
}

fn trim_web_sys_features_to_published(staging_dir: &Path) -> Result<()> {
    let manifest = staging_dir.join(WEB_SYS_MANIFEST);
    let published_features = published_crate_features(PUBLISHED_WEB_SYS_PACKAGE)?;
    let text = fs::read_to_string(&manifest)?;
    let features = parse_features(&text, &manifest)?;
    let local_names: BTreeSet<_> = features.iter().map(|feature| feature.name.as_str()).collect();

    let missing: Vec<_> = published_features
        .iter()
        .filter(|feature| !local_names.contains(feature.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(Error::new(format!(
            "{} is missing published {} feature(s): {}",
            manifest.display(),
            PUBLISHED_WEB_SYS_PACKAGE,
            missing.join(", ")
        )));
    }

    let retained = retained_web_sys_features(&features, &published_features);
    if retained.len() > CRATES_IO_FEATURE_LIMIT {
        return Err(Error::new(format!(
            "{} would publish {} features after trimming to {}; crates.io allows at most {}",
            manifest.display(),
            retained.len(),
            PUBLISHED_WEB_SYS_PACKAGE,
            CRATES_IO_FEATURE_LIMIT
        )));
    }

    let mut lines = Lines::from(&text);
    let (start, end) = lines.find_section_bounds("features").ok_or_else(|| {
        Error::new(format!(
            "{} is missing a [features] section",
            manifest.display()
        ))
    })?;

    let kept: Vec<_> = features
        .iter()
        .filter(|feature| retained.contains(&feature.name))
        .map(|feature| Line {
            body: feature.line.clone(),
            ending: "\n",
        })
        .collect();
    lines.lines.splice(start..end, kept);

    for index in 0..lines.len() {
        if lines
            .body(index)
            .contains("unexpected_cfgs = { level = \"warn\"")
        {
            lines.set_body(
                index,
                "unexpected_cfgs = { level = \"allow\", check-cfg = ['cfg(web_sys_unstable_apis)'] }"
                    .to_string(),
            );
        }
    }

    fs::write(manifest, lines.into_string())?;
    Ok(())
}

fn published_crate_features(package: &str) -> Result<BTreeSet<String>> {
    let output = Command::new("cargo")
        .args(["info", package, "--verbose", "--color", "never"])
        .output()
        .map_err(|error| Error::new(format!("failed to run `cargo info {package}`: {error}")))?;
    if !output.status.success() {
        return Err(Error::new(format!(
            "`cargo info {package} --verbose` failed with status {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| Error::new(format!("`cargo info {package}` emitted invalid UTF-8: {error}")))?;
    parse_cargo_info_features(&stdout, package)
}

fn parse_cargo_info_features(text: &str, package: &str) -> Result<BTreeSet<String>> {
    let mut in_features = false;
    let mut features = BTreeSet::new();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed == "features:" {
            in_features = true;
            continue;
        }
        if !in_features {
            continue;
        }
        if trimmed == "dependencies:" {
            break;
        }

        let trimmed = trimmed.strip_prefix('+').unwrap_or(trimmed).trim_start();
        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() {
            features.insert(name.to_string());
        }
    }

    if features.is_empty() {
        return Err(Error::new(format!(
            "could not parse any features from `cargo info {package} --verbose`"
        )));
    }

    Ok(features)
}

#[derive(Debug)]
struct Feature {
    name: String,
    dependencies: Vec<String>,
    line: String,
}

fn parse_features(text: &str, path: &Path) -> Result<Vec<Feature>> {
    let lines = Lines::from(text);
    let (start, end) = lines.find_section_bounds("features").ok_or_else(|| {
        Error::new(format!(
            "{} is missing a [features] section",
            path.display()
        ))
    })?;

    let mut features = Vec::new();
    for index in start..end {
        let line = lines.body(index);
        let Some(feature) = parse_feature_line(line) else {
            continue;
        };
        features.push(feature);
    }

    Ok(features)
}

fn parse_feature_line(line: &str) -> Option<Feature> {
    let (name, dependencies) = line.split_once('=')?;
    let name = name.trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }

    let mut dependencies = dependencies;
    let mut feature_dependencies = Vec::new();
    while let Some(start) = dependencies.find('"') {
        let rest = &dependencies[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let dependency = &rest[..end];
        if !dependency.contains('/') && !dependency.starts_with("dep:") {
            feature_dependencies.push(dependency.to_string());
        }
        dependencies = &rest[end + 1..];
    }

    Some(Feature {
        name: name.to_string(),
        dependencies: feature_dependencies,
        line: line.to_string(),
    })
}

fn retained_web_sys_features(
    features: &[Feature],
    published_features: &BTreeSet<String>,
) -> BTreeSet<String> {
    let dependencies_by_feature: BTreeMap<_, _> = features
        .iter()
        .map(|feature| (feature.name.as_str(), feature.dependencies.as_slice()))
        .collect();
    let local_names: BTreeSet<_> = features.iter().map(|feature| feature.name.as_str()).collect();
    let mut retained = published_features.clone();

    let mut changed = true;
    while changed {
        changed = false;
        for feature in retained.clone() {
            let Some(dependencies) = dependencies_by_feature.get(feature.as_str()) else {
                continue;
            };
            for dependency in *dependencies {
                if local_names.contains(dependency.as_str()) && retained.insert(dependency.clone()) {
                    changed = true;
                }
            }
        }
    }

    retained
}

fn merge_vendored_workspace_lints(staging_dir: &Path) -> Result<()> {
    let vendored_manifest = staging_dir.join("vendored/wasm-bindgen/Cargo.toml");
    let text = fs::read_to_string(&vendored_manifest)?;
    let lines = Lines::from(&text);
    let mut lint_sections = String::new();
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

        if name.starts_with("workspace.lints.") {
            for line in &lines.lines[start..index] {
                lint_sections.push_str(&line.body);
                lint_sections.push_str(line.ending);
            }
            lint_sections.push('\n');
        }
    }

    if lint_sections.is_empty() {
        return Err(Error::new(format!(
            "{} has no [workspace.lints.*] sections to preserve",
            vendored_manifest.display()
        )));
    }

    let root_manifest = staging_dir.join("Cargo.toml");
    let mut root = fs::read_to_string(&root_manifest)?;
    if !root.ends_with('\n') {
        root.push('\n');
    }
    root.push('\n');
    root.push_str(&lint_sections);
    fs::write(&root_manifest, root)?;
    fs::remove_file(vendored_manifest)?;
    Ok(())
}

fn remove_local_vendored_workspace_roots(staging_dir: &Path) -> Result<()> {
    for relative in ["vendored/wasm-bindgen/crates/macro-support/Cargo.toml"] {
        remove_manifest_sections(
            &staging_dir.join(relative),
            &["workspace", "workspace.lints.rust", "workspace.lints.clippy"],
        )?;
    }
    Ok(())
}

fn remove_manifest_sections(path: &Path, sections: &[&str]) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = Lines::from(&text);
    let mut changed = false;
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

        if sections.contains(&name) {
            lines.lines.drain(start..index);
            changed = true;
            index = start;
        }
    }

    if changed {
        fs::write(path, lines.into_string())?;
    }
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

    lines.replace_range(
        start - 1,
        end,
        &[
            "[workspace]",
            "members = [",
            "    \"packages/wry-bindgen\",",
            "    \"packages/wry-bindgen-core\",",
            "    \"packages/wry-bindgen-runtime\",",
            "    \"packages/wry-bindgen-macro\",",
            "    \"packages/wry-bindgen-macro-support\",",
            "    \"packages/wasm-bindgen\",",
            "    \"packages/wasm-bindgen-macro\",",
            "    \"vendored/wasm-bindgen/crates/macro-support\",",
            "    \"vendored/wasm-bindgen/crates/js-sys\",",
            "    \"vendored/wasm-bindgen/crates/web-sys\",",
            "    \"vendored/wasm-bindgen/crates/futures\",",
            "]",
            "exclude = [\"vendored/wasm-bindgen/crates/shared\"]",
            "resolver = \"2\"",
        ],
    );
    fs::write(path, lines.into_string())?;
    Ok(())
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

        // The wasm32 `wasm-bindgen` delegate is sourced from git so the workspace
        // `[patch.crates-io]` does not capture it. A git source cannot be published,
        // so repoint it at the tagged crates.io release: the published crate depends
        // on the real upstream `wasm-bindgen` (kept under its real name, not the `-x`
        // shim) at the same version as the tag.
        if inline_table_value(table_body, "git").is_some() {
            let package = inline_table_value(table_body, "package").unwrap_or(key);
            let version = inline_table_value(table_body, "tag").ok_or_else(|| {
                Error::new(format!(
                    "{}: git dependency `{key}` needs a `tag` to map to a crates.io version",
                    path.display()
                ))
            })?;
            lines.set_body(
                index,
                format!("{prefix} package = \"{package}\", version = \"{version}\" {suffix}"),
            );
            changed = true;
            continue;
        }

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

fn ensure_patch_crates_io_entry(
    path: &Path,
    dependency: &str,
    dependency_path: &str,
    package: &str,
) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut lines = Lines::from(&text);
    let (start, end) = lines.find_section_bounds("patch.crates-io").ok_or_else(|| {
        Error::new(format!(
            "{} is missing a [patch.crates-io] section",
            path.display()
        ))
    })?;

    for index in start..end {
        let line = lines.body(index).to_string();
        let Some((prefix, key, table_body, suffix)) = split_inline_table(&line) else {
            continue;
        };
        if key != dependency {
            continue;
        }

        let updated_table = upsert_inline_table_value(table_body, "path", dependency_path);
        let updated_table = upsert_inline_table_value(&updated_table, "package", package);
        lines.set_body(index, format!("{prefix}{updated_table}{suffix}"));
        fs::write(path, lines.into_string())?;
        return Ok(());
    }

    lines.insert(
        end,
        format!("{dependency} = {{ path = \"{dependency_path}\", package = \"{package}\" }}"),
    );
    fs::write(path, lines.into_string())?;
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

fn run_workspace_dry_run(
    staging_dir: &Path,
    publish_crates: &[PublishCrate],
    args: &Args,
) -> Result<()> {
    let mut command = Command::new("cargo");
    command.args(["publish", "--dry-run"]);
    if args.packages.is_empty() {
        command.arg("--workspace");
    } else {
        for krate in publish_crates {
            command.args(["--package", krate.publish_name]);
        }
    }
    if args.no_verify {
        command.arg("--no-verify");
    }
    command.current_dir(staging_dir);

    let status = command.status().map_err(|error| {
        Error::new(format!(
            "failed to run cargo publish --dry-run from {}: {error}",
            staging_dir.display()
        ))
    })?;
    if !status.success() {
        return Err(Error::new(format!(
            "workspace cargo publish --dry-run failed with status {status}",
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
        assert_eq!(
            renamed_package_name("wasm-bindgen-macro-support"),
            Some("wasm-bindgen-macro-support-x")
        );
        assert_eq!(renamed_package_name("js-sys"), Some("js-sys-x"));
        assert_eq!(renamed_package_name("serde"), None);
    }

    #[test]
    fn remove_manifest_sections_removes_matching_sections() {
        let path = env::temp_dir().join(format!(
            "publish-wasm-bindgen-x-section-test-{}.toml",
            process::id()
        ));
        let input = "\
[package]
name = \"wasm-bindgen-macro-support\"

[workspace]
resolver = \"2\"

[workspace.lints.rust]
unused_lifetimes = \"warn\"

[dependencies]
syn = \"2\"
";
        fs::write(&path, input).unwrap();

        remove_manifest_sections(&path, &["workspace", "workspace.lints.rust"]).unwrap();

        let output = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert!(output.contains("[package]"));
        assert!(output.contains("[dependencies]"));
        assert!(!output.contains("[workspace]"));
        assert!(!output.contains("[workspace.lints.rust]"));
    }

    #[test]
    fn ensure_patch_crates_io_entry_inserts_missing_entry() {
        let path = env::temp_dir().join(format!(
            "publish-wasm-bindgen-x-patch-test-{}.toml",
            process::id()
        ));
        let input = "\
[workspace]
members = []

[patch.crates-io]
wasm-bindgen = { path = \"packages/wasm-bindgen\", package = \"wasm-bindgen-x\" }
";
        fs::write(&path, input).unwrap();

        ensure_patch_crates_io_entry(
            &path,
            "web-sys",
            "vendored/wasm-bindgen/crates/web-sys",
            "web-sys-x",
        )
        .unwrap();

        let output = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert!(output.contains(
            "web-sys = { path = \"vendored/wasm-bindgen/crates/web-sys\", package = \"web-sys-x\" }"
        ));
    }

    #[test]
    fn ensure_patch_crates_io_entry_rewrites_existing_entry() {
        let path = env::temp_dir().join(format!(
            "publish-wasm-bindgen-x-patch-rewrite-test-{}.toml",
            process::id()
        ));
        let input = "\
[workspace]
members = []

[patch.crates-io]
web-sys = { path = \"vendored/wasm-bindgen/crates/web-sys\", package = \"web-sys\" }
";
        fs::write(&path, input).unwrap();

        ensure_patch_crates_io_entry(
            &path,
            "web-sys",
            "vendored/wasm-bindgen/crates/web-sys",
            "web-sys-x",
        )
        .unwrap();

        let output = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert!(output.contains(r#"package = "web-sys-x""#));
        assert!(!output.contains(r#"package = "web-sys""#));
    }
}
