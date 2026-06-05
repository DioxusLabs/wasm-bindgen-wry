//! Shared engine for the wry-launch test binaries.
//!
//! Both `main_thread_tests` and `upstream_tests` include this file via
//! `#[path = "../common/harness.rs"] mod harness;`. It owns everything generic — the test
//! model, the headless run loop, timeouts, JS-error reporting, and libtest-compatible
//! output — while each binary supplies its own `build_tests()`.

// Each binary uses a different subset of the engine (e.g. `upstream_tests` builds
// `TestCase`s directly rather than via `sync_test`/`async_test`).
#![allow(dead_code)]

use std::any::Any;
use std::future::Future;
use std::io::{self, Write};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use futures_util::{FutureExt, StreamExt, pin_mut, select};
use libtest_mimic::{Arguments, Failed};
use wasm_bindgen::Closure;
use wry_bindgen_runtime::wire::batch_async;
use wry_launch::set_on_error;

pub const TEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy)]
pub enum BatchMode {
    NonBatched,
    Batched,
}

impl BatchMode {
    pub fn suffix(self) -> &'static str {
        match self {
            BatchMode::NonBatched => "nonbatched",
            BatchMode::Batched => "batched",
        }
    }
}

#[derive(Clone, Copy, Default)]
struct HarnessOptions {
    repeat_for: Option<Duration>,
    quiet: bool,
}

// The futures returned by test bodies are !Send because they can hold
// Rc<RefCell<_>> values. They are polled on the single-threaded runtime where
// they were constructed.
pub type TestFuture = Pin<Box<dyn Future<Output = ()>>>;
pub type TestBody = Box<dyn FnOnce() -> TestFuture>;

pub struct TestCase {
    pub name: String,
    pub body: TestBody,
}

struct WallClockTimeoutGuard {
    done: Arc<AtomicBool>,
}

impl Drop for WallClockTimeoutGuard {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
    }
}

fn arm_wall_clock_timeout(timeout: Duration) -> WallClockTimeoutGuard {
    let done = Arc::new(AtomicBool::new(false));
    let done_for_thread = done.clone();
    let watchdog_timeout = timeout + Duration::from_secs(1);
    std::thread::spawn(move || {
        std::thread::sleep(watchdog_timeout);
        if !done_for_thread.load(Ordering::SeqCst) {
            eprintln!(
                "Test exceeded wall-clock timeout after {} seconds",
                watchdog_timeout.as_secs()
            );
            std::process::exit(101);
        }
    });

    WallClockTimeoutGuard { done }
}

pub async fn run_with_timeout(fut: impl Future<Output = ()>, mode: BatchMode, timeout: Duration) {
    let _wall_clock_timeout = arm_wall_clock_timeout(timeout);
    let body = async move {
        match mode {
            BatchMode::NonBatched => fut.await,
            BatchMode::Batched => batch_async(fut).await,
        }
    };
    tokio::select! {
        _ = body => {}
        _ = tokio::time::sleep(timeout) => {
            panic!("Test timed out after {} seconds", timeout.as_secs())
        }
    }
}

pub fn sync_test<F>(name: String, mode: BatchMode, f: F) -> TestCase
where
    F: Fn() + Copy + 'static,
{
    TestCase {
        name,
        body: Box::new(move || Box::pin(run_with_timeout(async move { f() }, mode, TEST_TIMEOUT))),
    }
}

pub fn async_test<Fut, F>(name: String, mode: BatchMode, f: F) -> TestCase
where
    F: Fn() -> Fut + Copy + 'static,
    Fut: Future<Output = ()> + 'static,
{
    TestCase {
        name,
        body: Box::new(move || Box::pin(run_with_timeout(f(), mode, TEST_TIMEOUT))),
    }
}

pub fn trial_name(module: &str, name: &str, mode: BatchMode) -> String {
    format!("{module}::{name}::{}", mode.suffix())
}

fn extract_panic_message(payload: Box<dyn Any + Send>) -> Failed {
    let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "test panicked".to_string()
    };
    Failed::from(msg)
}

fn install_js_error_reporter() -> UnboundedReceiver<String> {
    let (sender, receiver) = unbounded();
    set_on_error(Closure::new(move |err: String, stack: String| {
        let message = if stack.is_empty() {
            err
        } else {
            format!("{err}\nStack trace:\n{stack}")
        };
        let _ = sender.unbounded_send(message.clone());

        eprintln!("Fatal JavaScript error event:\n{message}");
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        std::process::exit(101);
    }));
    receiver
}

fn js_error_failure(message: String) -> Failed {
    Failed::from(format!("JavaScript error event:\n{message}"))
}

async fn run_test_body(body: TestBody) -> Result<(), Failed> {
    let fut = std::panic::catch_unwind(AssertUnwindSafe(body)).map_err(extract_panic_message)?;
    AssertUnwindSafe(fut)
        .catch_unwind()
        .await
        .map_err(extract_panic_message)
}

async fn run_test(body: TestBody, js_errors: &mut UnboundedReceiver<String>) -> Result<(), Failed> {
    if let Some(Some(message)) = js_errors.next().now_or_never() {
        return Err(js_error_failure(message));
    }

    let body = run_test_body(body).fuse();
    let js_error = js_errors.next().fuse();
    pin_mut!(body, js_error);

    select! {
        result = body => {
            if result.is_ok() {
                if let Some(Some(message)) = js_errors.next().now_or_never() {
                    return Err(js_error_failure(message));
                }
            }
            result
        }
        message = js_error => {
            let message = message.unwrap_or_else(|| "JavaScript error reporter stopped".to_string());
            Err(js_error_failure(message))
        }
    }
}

fn is_filtered_out(args: &Arguments, test: &TestCase) -> bool {
    if let Some(filter) = &args.filter {
        match args.exact {
            true if &test.name != filter => return true,
            false if !test.name.contains(filter) => return true,
            _ => {}
        }
    }

    for skip_filter in &args.skip {
        match args.exact {
            true if &test.name == skip_filter => return true,
            false if test.name.contains(skip_filter) => return true,
            _ => {}
        }
    }

    // This harness does not define ignored tests, so `--ignored` filters every
    // test out just like libtest would.
    args.ignored
}

fn print_failures(failures: &[(String, Option<String>)]) {
    if failures.is_empty() {
        return;
    }

    println!();
    println!("failures:");
    println!();

    for (name, message) in failures {
        println!("---- {name} ----");
        if let Some(message) = message {
            println!("{message}");
        }
        println!();
    }

    println!();
    println!("failures:");
    for (name, _) in failures {
        println!("    {name}");
    }
}

async fn run_tests(
    args: Arguments,
    mut tests: Vec<TestCase>,
    js_errors: &mut UnboundedReceiver<String>,
    quiet: bool,
) -> bool {
    let started = Instant::now();
    let initial_count = tests.len();
    tests.retain(|test| !is_filtered_out(&args, test));
    let filtered = initial_count - tests.len();

    if args.list {
        for test in tests {
            println!("{}: test", test.name);
        }
        return true;
    }

    let plural = if tests.len() == 1 { "" } else { "s" };
    if !quiet {
        println!();
        println!("running {} test{plural}", tests.len());
    }

    let name_width = tests
        .iter()
        .map(|test| test.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut passed = 0;
    let mut ignored = 0;
    let mut failures = Vec::new();

    for test in tests {
        if !quiet {
            print!("test {: <name_width$} ... ", test.name);
            io::stdout().flush().unwrap();
        }

        if args.bench {
            ignored += 1;
            if !quiet {
                println!("ignored");
            }
            continue;
        }

        match run_test(test.body, js_errors).await {
            Ok(()) => {
                passed += 1;
                if !quiet {
                    println!("ok");
                }
            }
            Err(failed) => {
                if !quiet {
                    println!("FAILED");
                }
                failures.push((test.name.clone(), failed.message().map(ToOwned::to_owned)));
            }
        }
    }

    print_failures(&failures);

    let result = if failures.is_empty() { "ok" } else { "FAILED" };
    if !quiet || !failures.is_empty() {
        println!();
        println!(
            "test result: {result}. {passed} passed; {} failed; {ignored} ignored; 0 measured; {filtered} filtered out; finished in {:.2}s",
            failures.len(),
            started.elapsed().as_secs_f64(),
        );
        println!();
    }

    failures.is_empty()
}

fn parse_harness_args() -> (HarnessOptions, Vec<String>) {
    let mut options = HarnessOptions::default();
    let mut libtest_args = Vec::new();

    let mut args = std::env::args();
    if let Some(executable) = args.next() {
        libtest_args.push(executable);
    }

    for arg in args {
        if let Some(value) = arg.strip_prefix("--wry-repeat-for-secs=") {
            options.repeat_for = value.parse::<u64>().ok().map(Duration::from_secs);
        } else if arg == "--wry-quiet" {
            options.quiet = true;
        } else {
            libtest_args.push(arg);
        }
    }

    if options.repeat_for.is_none() {
        options.repeat_for = std::env::var("WRY_BINDGEN_REPEAT_FOR_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_secs);
    }

    (options, libtest_args)
}

/// Run the full harness for a binary that supplies `build_tests`. Mirrors the libtest
/// runner: parses args, boots a headless wry webview, optionally repeats, and exits with
/// the libtest status code.
pub fn harness_main(build_tests: fn() -> Vec<TestCase>) -> ExitCode {
    let (options, libtest_args) = parse_harness_args();
    let args = Arguments::from_iter(libtest_args);

    wry_launch::run_headless(move || async move {
        let mut js_errors = install_js_error_reporter();
        let passed = if let Some(duration) = options.repeat_for.filter(|_| !args.list) {
            let start = Instant::now();
            let mut iteration = 0u64;

            loop {
                iteration += 1;
                if !options.quiet {
                    println!(
                        "=== test iteration {iteration} elapsed {:?} ===",
                        start.elapsed()
                    );
                }

                if !run_tests(args.clone(), build_tests(), &mut js_errors, options.quiet).await {
                    break false;
                }

                if start.elapsed() >= duration {
                    println!(
                        "=== completed {iteration} clean test iterations in {:?} ===",
                        start.elapsed()
                    );
                    break true;
                }
            }
        } else {
            run_tests(args, build_tests(), &mut js_errors, options.quiet).await
        };

        let _ = io::stdout().flush();
        let _ = io::stderr().flush();
        std::process::exit(if passed { 0 } else { 101 });
    })
    .expect("failed to run headless test harness");

    ExitCode::SUCCESS
}
