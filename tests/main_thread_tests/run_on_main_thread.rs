use wry_testing::run_on_main_thread;

/// Test basic execution: closure runs, blocks until complete, and returns value.
pub(crate) fn test_run_on_main_thread_basic() {
    // If we get the return value, the closure executed and we blocked until completion
    let result: u32 = run_on_main_thread(|| 42);
    assert_eq!(result, 42);

    // Test with captured value
    let input = 10u32;
    let result: u32 = run_on_main_thread(move || input * 2);
    assert_eq!(result, 20);
}
