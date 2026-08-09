// Proof-of-concept: use iced_test::Emulator (not Simulator) to drive a real
// keyboard-focus round trip for spe-0g1 ("focus returns to the floating
// editor after a toolbar font change").
//
// Why this exists: iced_test::Simulator applies widget events synchronously
// but never runs the runtime Tasks/operations a `Program::update` returns.
// The floating editor's refocus (App::refocus_editing_widget, called from
// handle_change_font) is implemented as an `iced::widget::operation::focus`
// Task, so Simulator can observe the *unfocus* caused by clicking the
// toolbar but not the refocus that follows. iced_test::Emulator runs a real
// iced_runtime::Runtime and does perform Tasks/widget operations (see
// iced_test::emulator::Emulator::perform's `runtime::Action::Widget(operation)`
// arm, iced_test-0.14.0/src/emulator.rs:185-208, which executes the operation
// against a live UserInterface), so it can observe the full round trip.
//
// Two real limitations discovered while building this, both worth recording:
//
// 1. Reading focus back out. There is no supported public API to read
//    arbitrary widget state (e.g. `Focusable::is_focused`) from an
//    `Emulator` after driving it. `Emulator::run` only exposes
//    `Instruction::Expect(Expectation::Text)` (iced_test-0.14.0/src/emulator.rs:351-373),
//    matched against the emulator's own live, persisted widget-tree cache.
//    Building a *second*, independent `UserInterface` to query focus
//    directly does not work: it starts from a fresh `Cache::default()`,
//    which has no memory of the mutations the emulator applied (verified
//    empirically against a standalone UserInterface -- see the debug
//    experiment referenced in the eval report). So this test observes the
//    *behavioral* consequence of focus instead: iced's text_input widget
//    only consumes character key-presses while its internal `is_focused`
//    state is set (iced_widget-0.14.2/src/text_input.rs:890, the `update`
//    match arm gated on `if let Some(focus) = &mut state.is_focused`).
//    Typing a character and checking whether it lands in the overlay is an
//    exact, fully-supported proxy for "is the floating editor focused" --
//    and it's literally what spe-0g1 is about (can the user type).
//
// 2. Targeting the font pick_list by text does not work. `Instruction`'s
//    text-based click target relies on `Selector::find`, which walks the
//    widget tree via `Widget::operate` (iced_test-0.14.0/src/emulator.rs:296-321).
//    `iced_widget::pick_list::PickList` has no `operate` override
//    (confirmed absent in iced_widget-0.14.2/src/pick_list.rs), so its
//    displayed value is invisible to text-based targeting -- `click
//    "Helvetica"` cannot find the font picker at all. There is also no
//    `.id(...)` on it to target by Id, and no supported way to query its
//    on-screen bounds to click by point either. So the font-change message
//    (`Message::Toolbar(toolbar::Message::FontSelected(..))`, exactly what
//    a real pick_list selection emits) is dispatched directly via
//    `Emulator::update`, not through a synthesized pointer click. The
//    toolbar click *is* simulated for real, by clicking the "100%" zoom
//    label instead -- any real pointer click outside the floating editor
//    reproduces the same runtime-level unfocus a font-picker click would,
//    without depending on pick_list's non-existent hit-testing support.
//
// `Emulator::update` (unlike `Emulator::run`) does not chain a terminal
// `Ready` event when called from outside an active `wait_for` (see
// `Emulator::update`, iced_test-0.14.0/src/emulator.rs:140-161: the
// `_ =>` arm just spawns the resulting stream without the
// `Ready`-chaining `wait_for` does). So driving it needs a short bounded
// drain instead of `drain_until_ready`'s blocking wait.
//
// This test needs a working headless renderer (GPU-via-Mesa or the
// tiny-skia software fallback) and is marked #[ignore] for the same reason
// tests/e2e.rs's Simulator-based tests are: CI runs `cargo test --
// --ignored` with a Mesa llvmpipe software rasterizer available; plain
// `cargo test` environments may not have one (this sandbox does, via the
// tiny-skia fallback -- see the eval report).

use std::time::{Duration, Instant};

use iced_test::Instruction;
use iced_test::emulator::{Emulator, Event, Mode};
use iced_test::futures::futures::StreamExt;
use iced_test::futures::futures::channel::mpsc;
use iced_test::futures::futures::executor as futures_executor;
use iced_test::program::{Preset, Program};

use spe::app::{App, DocumentState, Message};
use spe::overlay::PdfPosition;
use spe::ui::toolbar;

/// Boots an `App` with a document loaded and a single-line overlay under
/// active edit, mirroring `handle_place_overlay`'s real effects (including
/// the focus Task it returns).
fn editing_preset() -> Preset<App, Message> {
    Preset::new("editing", || {
        let (mut app, boot_task) = App::new(false);
        // The sidebar's shimmer-tick subscription fires every 16ms while
        // visible with unrendered thumbnails. The Emulator runs real
        // subscriptions (unlike Simulator), so leaving it on would make
        // this test's message stream never go quiet. Not needed for a
        // focus test, so turn it off.
        app.sidebar.visible = false;
        app.document = Some(DocumentState {
            source_path: "/tmp/test.pdf".into(),
            save_path: None,
            page_count: 1,
            current_page: 1,
            page_images: Default::default(),
            page_dimensions: [(1, (612.0, 792.0))].into_iter().collect(),
            overlays: Vec::new(),
        });

        let place_task = app.update(Message::PlaceOverlay {
            page: 1,
            position: PdfPosition { x: 100.0, y: 700.0 },
            width: None,
        });

        (app, iced::Task::batch([boot_task, place_task]))
    })
}

/// Pumps the emulator's event channel, performing every `Action` it emits,
/// until it reports `Ready`. Panics on `Failed` so a mis-targeted
/// instruction shows up as a test failure instead of a silent no-op.
fn drain_until_ready<P: Program + 'static>(
    emulator: &mut Emulator<P>,
    program: &P,
    receiver: &mut mpsc::Receiver<Event<P>>,
) {
    loop {
        let event = futures_executor::block_on(receiver.next())
            .expect("emulator runtime should never stop on its own");

        match event {
            Event::Action(action) => emulator.perform(program, action),
            Event::Failed(instruction) => {
                panic!("instruction failed to execute: {instruction}")
            }
            Event::Ready => break,
        }
    }
}

/// Runs an [`Instruction`] and drains the emulator to `Ready`, so every Task
/// / widget operation it produces (including nested ones) has finished
/// before the next instruction is issued.
fn step<P: Program + 'static>(
    emulator: &mut Emulator<P>,
    program: &P,
    receiver: &mut mpsc::Receiver<Event<P>>,
    instruction: &str,
) {
    emulator.run(program, Instruction::parse(instruction).unwrap());
    drain_until_ready(emulator, program, receiver);
}

/// Dispatches a `Message` directly (not through a synthesized UI event) and
/// drains whatever it produces. See the module doc comment for why this is
/// needed for the font pick_list specifically.
fn dispatch<P: Program + 'static>(
    emulator: &mut Emulator<P>,
    program: &P,
    receiver: &mut mpsc::Receiver<Event<P>>,
    message: P::Message,
) {
    emulator.update(program, message);

    let quiet_for = Duration::from_millis(20);
    let mut last_event = Instant::now();

    while last_event.elapsed() < quiet_for {
        match receiver.try_recv() {
            Ok(Event::Action(action)) => {
                emulator.perform(program, action);
                last_event = Instant::now();
            }
            Ok(Event::Failed(instruction)) => {
                panic!("instruction failed to execute: {instruction}")
            }
            Ok(Event::Ready) => last_event = Instant::now(),
            Err(_) => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}

#[test]
#[ignore] // needs a working headless renderer (GPU/software fallback); see tests/e2e.rs
fn font_change_from_toolbar_restores_floating_editor_focus() {
    let size = iced::Size::new(1200.0, 1600.0);

    let program = iced::application(|| App::new(false), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription);

    let preset = editing_preset();

    let (sender, mut receiver) = mpsc::channel(64);
    let mut emulator = Emulator::with_preset(sender, &program, Mode::Zen, size, Some(&preset));

    // Drain the boot events: by the time this reports Ready, the boot task
    // -- including the PlaceOverlay handler's focus Task -- has fully run.
    drain_until_ready(&mut emulator, &program, &mut receiver);

    // Sanity check: the floating editor is focused right after placement,
    // so typing "A" should land in it.
    step(&mut emulator, &program, &mut receiver, "type \"A\"");
    step(&mut emulator, &program, &mut receiver, "expect \"A\"");

    // Simulate a real pointer click on the toolbar, away from the floating
    // editor (the "100%" zoom label). This reproduces the same
    // runtime-level "click elsewhere unfocuses the current widget"
    // behavior a font-picker click would -- exactly what Simulator could
    // already show.
    step(&mut emulator, &program, &mut receiver, "click \"100%\"");

    // While unfocused, typing "X" must NOT land in the overlay.
    step(&mut emulator, &program, &mut receiver, "type \"X\"");

    // Dispatch the message a real font selection would send. This runs
    // handle_toolbar_message -> handle_change_font -> the refocus Task --
    // the part Simulator cannot run.
    let font = toolbar::font_options(&App::new(false).0.font_registry)
        .into_iter()
        .find(|option| option.name == "Courier")
        .expect("Courier should be a registered font");

    dispatch(
        &mut emulator,
        &program,
        &mut receiver,
        Message::Toolbar(toolbar::Message::FontSelected(font)),
    );

    // Now focused again: typing "B" should land right after the "A",
    // proving the "X" typed while unfocused was dropped and the refocus
    // Task actually ran.
    step(&mut emulator, &program, &mut receiver, "type \"B\"");
    step(&mut emulator, &program, &mut receiver, "expect \"AB\"");
}
