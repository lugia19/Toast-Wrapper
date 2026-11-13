//! Test DLL for client\ac6_toast_client.rs: 20 s after load, shows "client OK (API n)" through ac6_toast.dll, and
//! logs the result to Game\debug-logs\ac6_toast_client_test.log (also when ac6_toast.dll isn't loaded).

#[path = "../../../client/ac6_toast_client.rs"]
mod ac6_toast_client;

fn run() {
    std::thread::sleep(std::time::Duration::from_secs(20));
    let version = ac6_toast_client::api_version();
    let toast = version.and_then(|v| ac6_toast_client::show(&format!("client OK (API {v})"), 5.0));
    let line = format!("api_version {version:?}, show -> {toast:?}\n");
    let log = std::env::current_exe().unwrap().with_file_name("debug-logs").join("ac6_toast_client_test.log");
    let _ = std::fs::write(log, line);
}

#[unsafe(no_mangle)]
extern "system" fn DllMain(_module: *mut std::ffi::c_void, reason: u32, _reserved: *mut std::ffi::c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_ATTACH {
        std::thread::spawn(run);
    }
    1
}
