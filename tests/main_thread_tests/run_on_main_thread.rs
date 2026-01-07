use std::thread::ThreadId;
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

/// Test that the closure actually executes on the main thread by comparing thread IDs.
pub(crate) fn test_run_on_main_thread_verifies_thread() {
    let main_thread_id: ThreadId = run_on_main_thread(|| std::thread::current().id());

    // From a background thread, verify the closure runs on the same main thread
    let handle = std::thread::spawn(move || {
        let id_from_closure: ThreadId = run_on_main_thread(|| std::thread::current().id());
        assert_eq!(
            id_from_closure, main_thread_id,
            "Closure should execute on main thread"
        );

        let current_thread_id = std::thread::current().id();
        assert_ne!(
            current_thread_id, main_thread_id,
            "Spawned thread should differ from main thread"
        );
    });
    handle.join().expect("Thread should not panic");
}
