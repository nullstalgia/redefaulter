#![allow(unused)]

use tray_icon::{
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIconBuilder, TrayIconEvent,
};
use winit::event_loop::{ControlFlow, EventLoopBuilder};

// load config
// get current audio devices
// check against devices and change any outliers if possible
// check for violation of unify comms
// set up windows callback channels
// event loop listens for changes in audio devices (connect/disconnect/default change)
// event loop listens for specified processes opening/closing (debounce?)
// regenerate menu each time? or on click...
fn main() {
    let path = "icon.png";

    let event_loop = EventLoopBuilder::new().build().unwrap();

    let mut tray_icon = None;

    let tray_menu = Menu::new();

    let quit_i = MenuItem::new("Quit", true, None);
    tray_menu.append_items(&[
        &PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("tao".to_string()),
                copyright: Some("Copyright tao".to_string()),
                ..Default::default()
            }),
        ),
        &PredefinedMenuItem::separator(),
        &quit_i,
    ]);

    let menu_channel = MenuEvent::receiver();
    let tray_channel = TrayIconEvent::receiver();

    event_loop.run(move |event, event_loop| {
        // We add delay of 16 ms (60fps) to event_loop to reduce cpu load.
        // This can be removed to allow ControlFlow::Poll to poll on each cpu cycle
        // Alternatively, you can set ControlFlow::Wait or use TrayIconEvent::set_event_handler,
        // see https://github.com/tauri-apps/tray-icon/issues/83#issuecomment-1697773065
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(16),
        ));

        if let winit::event::Event::NewEvents(winit::event::StartCause::Init) = event {
            let icon = load_icon(std::path::Path::new(path));

            // We create the icon once the event loop is actually running
            // to prevent issues like https://github.com/tauri-apps/tray-icon/issues/90
            tray_icon = Some(
                TrayIconBuilder::new()
                    .with_menu(Box::new(tray_menu.clone()))
                    .with_tooltip("winit - awesome windowing lib")
                    .with_icon(icon)
                    .with_title("x")
                    .build()
                    .unwrap(),
            );
        }

        if let Ok(event) = menu_channel.try_recv() {
            if event.id == quit_i.id() {
                println!("Quit clicked!");
                event_loop.exit();
            }
            println!("{event:?}");
        }

        if let Ok(event) = tray_channel.try_recv() {
            println!("{event:?}");
        }
    });
}

fn load_icon(path: &std::path::Path) -> tray_icon::Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::open(path)
            .expect("Failed to open icon path")
            .into_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    tray_icon::Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to open icon")
}
