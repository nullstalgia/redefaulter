use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use color_eyre::eyre::Result;
use tracing::*;

use crate::app::{AppEventProxy, CustomEvent};

// https://ratatui.rs/recipes/apps/better-panic/
pub fn initialize_panic_handler() -> Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default()
        .panic_section(format!(
            "This is a bug. Consider reporting it at {}",
            env!("CARGO_PKG_REPOSITORY")
        ))
        .display_location_section(true)
        .display_env_section(true)
        .into_hooks();
    eyre_hook.install()?;
    std::panic::set_hook(Box::new(move |panic_info| {
        error!("Panic! {:#?}", panic_info);
        let msg = format!("{}", panic_hook.panic_report(panic_info));
        #[cfg(not(debug_assertions))]
        {
            eprintln!("{}", msg); // prints color-eyre stack trace to stderr
            use human_panic::{handle_dump, print_msg, Metadata};
            let meta = Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

            let file_path = handle_dump(&meta, panic_info);
            // prints human-panic message
            print_msg(file_path.clone(), &meta)
                .expect("human-panic: printing error message to console failed");

            info!("Full panic dump at: {:?}", file_path);
        }
        eprintln!("Error: {}", strip_ansi_escapes::strip_str(msg));

        #[cfg(debug_assertions)]
        {
            // Better Panic stacktrace that is only enabled when debugging.
            better_panic::Settings::auto()
                .most_recent_first(false)
                .lineno_suffix(true)
                .verbosity(better_panic::Verbosity::Full)
                .create_panic_handler()(panic_info);
        }

        std::process::exit(libc::EXIT_FAILURE);
    }));
    Ok(())
}

pub fn initialize_ctrl_c_handler(proxy: AppEventProxy) -> Result<()> {
    let running = Arc::new(AtomicUsize::new(0));
    ctrlc::set_handler(move || {
        let prev = running.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            info!("Exiting via Ctrl+C");
            // If fails, event loop is already closed.
            let _ = proxy.send_event(CustomEvent::ExitRequested);
        } else {
            warn!("Forcibly exiting via Ctrl+C!");
            std::process::exit(libc::EXIT_FAILURE);
        }
    })?;
    Ok(())
}
