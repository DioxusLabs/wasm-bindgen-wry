#!/bin/sh
//usr/bin/env rustc --edition=2024 "$0" -o "${TMPDIR:-/tmp}/check-wasm-bindgen-releases" && CHECK_WASM_BINDGEN_RELEASES_SOURCE="$0" "${TMPDIR:-/tmp}/check-wasm-bindgen-releases" "$@"; exit $?

use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const DEFAULT_REMOTE: &str = "https://github.com/wasm-bindgen/wasm-bindgen.git";
const UPSTREAM_PACKAGE_NAMES: &[&str] = &["wasm-bindgen", "not-wasm-bindgen"];

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
    check: bool,
    list: bool,
    current: Option<String>,
    remote: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

fn main() {
    match run() {
        Ok(exit_code) => process::exit(exit_code),
        Err(error) => {
            eprintln!("error: {error}");
            process::exit(2);
        }
    }
}

fn run() -> Result<i32> {
    let args = parse_args()?;
    let current = match args.current.as_deref() {
        Some(version) => parse_version(version)
            .ok_or_else(|| Error::new(format!("invalid --current version `{version}`")))?,
        None => read_current_version()?,
    };
    let remote = args.remote.as_deref().unwrap_or(DEFAULT_REMOTE);
    let releases = fetch_releases(remote)?;
    let latest = releases.last().copied().ok_or_else(|| {
        Error::new(format!(
            "no stable wasm-bindgen release tags found in {remote}"
        ))
    })?;
    let newer: Vec<_> = releases
        .iter()
        .copied()
        .filter(|version| *version > current)
        .collect();

    println!("current wasm-bindgen version: {current}");
    println!("latest upstream release:     {latest}");

    if newer.is_empty() {
        println!("status: up to date");
        return Ok(0);
    }

    if args.list {
        let versions = newer
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        println!("new upstream releases:      {versions}");
    } else {
        println!("new upstream release:       {latest}");
    }

    Ok(if args.check { 1 } else { 0 })
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
            "--check" => args.check = true,
            "--list" => args.list = true,
            "--current" => {
                args.current = Some(
                    raw.next()
                        .ok_or_else(|| Error::new("--current requires a value"))?,
                );
            }
            "--remote" => {
                args.remote = Some(
                    raw.next()
                        .ok_or_else(|| Error::new("--remote requires a value"))?,
                );
            }
            _ => {
                if let Some(current) = arg.strip_prefix("--current=") {
                    args.current = Some(current.to_string());
                } else if let Some(remote) = arg.strip_prefix("--remote=") {
                    args.remote = Some(remote.to_string());
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
  check-wasm-bindgen-releases [--check] [--list] [--current VERSION] [--remote URL]

Options:
  --check            Exit 1 when a newer upstream release exists.
  --list             Print every newer release, not just the latest.
  --current VERSION  Compare against VERSION instead of wasm-bindgen/Cargo.toml.
  --remote URL       Query a different git remote. Defaults to official upstream.

The script reads stable release tags from upstream git refs and ignores prerelease tags."
    );
}

fn read_current_version() -> Result<Version> {
    let repo_root = repo_root()?;
    let manifest = repo_root.join("wasm-bindgen/Cargo.toml");
    let text = fs::read_to_string(&manifest)?;
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
    if !UPSTREAM_PACKAGE_NAMES.contains(&name) {
        return Err(Error::new(format!(
            "{} package name is `{name}`, expected one of: {}",
            manifest.display(),
            UPSTREAM_PACKAGE_NAMES.join(", ")
        )));
    }
    if name != "wasm-bindgen" {
        eprintln!(
            "note: reading wasm-bindgen version from patched upstream package `{name}` in {}",
            manifest.display()
        );
    }

    let version = read_field(package, "version").ok_or_else(|| {
        Error::new(format!(
            "{} is missing package field `version`",
            manifest.display()
        ))
    })?;
    parse_version(version).ok_or_else(|| {
        Error::new(format!(
            "{} package version `{version}` is not a stable semver release",
            manifest.display()
        ))
    })
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
    let Ok(source_path) = env::var("CHECK_WASM_BINDGEN_RELEASES_SOURCE") else {
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

fn fetch_releases(remote: &str) -> Result<Vec<Version>> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", remote, "refs/tags/*"])
        .output()
        .map_err(|error| Error::new(format!("failed to query {remote}: {error}")))?;
    if !output.status.success() {
        return Err(Error::new(format!(
            "failed to query {remote}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let mut releases = parse_releases(&String::from_utf8_lossy(&output.stdout));
    releases.sort_unstable();
    releases.dedup();
    Ok(releases)
}

fn parse_releases(output: &str) -> Vec<Version> {
    output
        .lines()
        .filter_map(|line| line.split_once("refs/tags/").map(|(_, tag)| tag))
        .filter_map(parse_version)
        .collect()
}

fn parse_version(version: &str) -> Option<Version> {
    let version = version.strip_prefix('v').unwrap_or(version);
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_versions() {
        assert_eq!(
            parse_version("0.2.122"),
            Some(Version {
                major: 0,
                minor: 2,
                patch: 122
            })
        );
        assert_eq!(parse_version("v0.2.123").unwrap().to_string(), "0.2.123");
    }

    #[test]
    fn ignores_prerelease_and_build_versions() {
        assert_eq!(parse_version("0.2.122-alpha.1"), None);
        assert_eq!(parse_version("0.2.122+build"), None);
        assert_eq!(parse_version("0.2"), None);
    }

    #[test]
    fn parses_and_sorts_release_tags() {
        let output = "\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trefs/tags/0.2.120
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\trefs/tags/0.2.122
cccccccccccccccccccccccccccccccccccccccc\trefs/tags/0.2.121
dddddddddddddddddddddddddddddddddddddddd\trefs/tags/0.2.122-alpha.1
eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\trefs/tags/v0.2.123
";
        let mut releases = parse_releases(output);
        releases.sort_unstable();

        assert_eq!(
            releases.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["0.2.120", "0.2.121", "0.2.122", "0.2.123"]
        );
    }

    #[test]
    fn version_order_is_numeric() {
        assert!(parse_version("0.2.100").unwrap() > parse_version("0.2.99").unwrap());
        assert!(parse_version("0.3.0").unwrap() > parse_version("0.2.999").unwrap());
    }

    #[test]
    fn accepts_patched_upstream_package_name() {
        assert!(UPSTREAM_PACKAGE_NAMES.contains(&"wasm-bindgen"));
        assert!(UPSTREAM_PACKAGE_NAMES.contains(&"not-wasm-bindgen"));
    }
}
