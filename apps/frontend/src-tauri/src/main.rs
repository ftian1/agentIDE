// Debug: use console subsystem so errors are visible from cmd.
// Change back to "windows" for release builds once stable.
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![cfg_attr(not(debug_assertions), windows_subsystem = "console")]

fn main() {
    // Set up early panic hook — writes crash info to a file so it's
    // visible even when running as a GUI (windows) subsystem.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Try to write crash info to a known location
        let crash_msg = format!(
            "Remote AI IDE crashed:\n\
             Location: {:?}\n\
             Message: {}\n",
            info.location(),
            info.payload()
                .downcast_ref::<&str>()
                .unwrap_or(&info.payload().downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .unwrap_or("(unknown)"))
        );
        let _ = std::fs::write("remote-ai-ide-crash.log", &crash_msg);
        eprintln!("{}", crash_msg);
        hook(info);
    }));

    remote_ai_ide_lib::run()
}
