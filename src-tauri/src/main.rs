// DO NOT use windows subsystem, we want to be a console app by default to block the shell
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        // If NO arguments are passed (just the executable), it's likely a GUI launch.
        // We detach from the console (FreeConsole) to avoid blocking the shell or showing a window 
        // if executed from a double-click.
        if std::env::args().count() == 1 {
            unsafe {
                windows_sys::Win32::System::Console::FreeConsole();
            }
        }
    }
    
    daily_assistant_lib::run()
}
