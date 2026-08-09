fn main() -> iced::Result {
    let ipc_enabled = std::env::args().any(|a| a == "--ipc");
    // Refuse --ipc up front rather than launching a window whose IPC socket
    // silently never appears, leaving an automation client waiting forever.
    if ipc_enabled && let Err(e) = spe::ipc::socket_path() {
        eprintln!("spe: --ipc requires a private runtime directory. {e}");
        std::process::exit(1);
    }
    iced::application(
        move || spe::app::App::new(ipc_enabled),
        spe::app::App::update,
        spe::app::App::view,
    )
    .title(spe::app::App::title)
    .subscription(spe::app::App::subscription)
    .run()
}
