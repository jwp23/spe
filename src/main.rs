fn main() -> iced::Result {
    let ipc_enabled = std::env::args().any(|a| a == "--ipc");
    iced::application(
        move || spe::app::App::new(ipc_enabled),
        spe::app::App::update,
        spe::app::App::view,
    )
    .title(spe::app::App::title)
    .subscription(spe::app::App::subscription)
    .run()
}
