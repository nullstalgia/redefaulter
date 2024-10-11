#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::AudioNightmare;

// I don't plan on doing this, but I'd rather over-engineer a little to prevent either myself
// or someone else some future pain.
// #[cfg(target_os = "linux")]
// mod unix;
// #[cfg(target_os = "linux")]
// pub use unix::AudioNightmare;
