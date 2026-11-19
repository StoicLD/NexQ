// Tauri isolates the database by identifier. Use the same profile for paths
// and Windows Credential Manager keys that are resolved without an AppHandle.
#[cfg(all(windows, feature = "windows-dev-profile"))]
pub const APP_ID: &str = "com.nexq.dev";
#[cfg(not(all(windows, feature = "windows-dev-profile")))]
pub const APP_ID: &str = "com.nexq.app";

#[cfg(all(windows, feature = "windows-dev-profile"))]
pub const APP_NAME: &str = "NexQDev";
#[cfg(not(all(windows, feature = "windows-dev-profile")))]
pub const APP_NAME: &str = "NexQ";
