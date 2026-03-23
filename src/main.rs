fn main() -> iced::Result {
    iced::application(
        spe::app::App::new,
        spe::app::App::update,
        spe::app::App::view,
    )
    .title(spe::app::App::title)
    .subscription(spe::app::App::subscription)
    .run()
}
