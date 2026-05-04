#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(windows, target_os = "linux")))]
mod other;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(not(any(windows, target_os = "linux")))]
pub use other::*;
#[cfg(windows)]
pub use windows::*;
