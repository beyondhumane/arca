use windows::core::w;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

const BROADCAST_TIMEOUT_MS: u32 = 5000;

/// Tells every top-level window that the user environment changed.
///
/// A window that does not answer within the timeout is skipped instead of
/// waited for: one hung program must not hold the installer.
pub fn environment_changed() {
    let scope = w!("Environment");
    // SAFETY: `scope` is a NUL-terminated wide string that outlives the call,
    // and WM_SETTINGCHANGE only reads it during the call.
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(scope.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            BROADCAST_TIMEOUT_MS,
            None,
        );
    }
}

/// Tells the shell to re-read every file association.
pub fn associations_changed() {
    // SAFETY: with SHCNE_ASSOCCHANGED both item pointers are ignored, and
    // passing none is what the documentation asks for.
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}
