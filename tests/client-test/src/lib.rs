//! Test DLL for client\toast_wrapper_client.rs: 20 s after load, shows "client OK (API n)" through toast_wrapper.dll,
//! and logs the result to Game\debug-logs\toast_wrapper_client_test.log (also when toast_wrapper.dll isn't loaded).

#[path = "../../../client/toast_wrapper_client.rs"]
mod toast_wrapper_client;

fn run() {
    std::thread::sleep(std::time::Duration::from_secs(20));
    let version = toast_wrapper_client::api_version();
    let shown = version.is_some_and(|v| toast_wrapper_client::show(&format!("client OK (API {v})")));
    let line = format!("api_version {version:?}, show -> {shown}\n");
    let log = std::env::current_exe().unwrap().with_file_name("debug-logs").join("toast_wrapper_client_test.log");
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
