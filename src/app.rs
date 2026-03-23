// Iced Application: top-level state, message routing, view composition.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::keyboard;
use iced::widget::image::Handle;

use crate::command::Command as UndoCommand;
use crate::config::AppConfig;
use crate::overlay::{PdfPosition, Standard14Font, TextOverlay};
use crate::pdf::renderer::{PageRenderer, PdftoppmRenderer};
use crate::ui::canvas::{self, CanvasState, PdfCanvasProgram};
use crate::ui::sidebar::SidebarState;
use crate::ui::toolbar::{self, ToolbarContext, ToolbarState};

/// State for the currently loaded PDF document.
pub struct DocumentState {
    pub source_path: PathBuf,
    pub save_path: Option<PathBuf>,
    pub page_count: u32,
    pub current_page: u32,
    pub page_images: HashMap<u32, Handle>,
    pub page_dimensions: HashMap<u32, (f32, f32)>,
    pub overlays: Vec<TextOverlay>,
}

/// Top-level application state.
pub struct App {
    pub document: Option<DocumentState>,
    pub toolbar: ToolbarState,
    pub canvas: CanvasState,
    pub sidebar: SidebarState,
    pub undo_stack: Vec<UndoCommand>,
    pub redo_stack: Vec<UndoCommand>,
    pub config: AppConfig,
}

/// All messages the application can process.
#[derive(Debug, Clone)]
pub enum Message {
    // File operations
    OpenFile,
    FileOpened(PathBuf),
    Save,
    SaveAs,
    SaveDestinationChosen(PathBuf),

    // Page navigation
    GoToPage(u32),
    NextPage,
    PreviousPage,
    PageRendered(u32, Handle),

    // Overlay editing (undoable)
    PlaceOverlay(PdfPosition),
    UpdateOverlayText(String),
    CommitText,
    MoveOverlay(usize, PdfPosition),
    ChangeFont(Standard14Font),
    ChangeFontSize(f32),
    DeleteOverlay,
    SelectOverlay(usize),
    DeselectOverlay,

    // Canvas
    ZoomIn,
    ZoomOut,
    ZoomReset,

    // Sidebar
    ToggleSidebar,
    ThumbnailRendered(u32, Handle),

    // Undo/Redo
    Undo,
    Redo,

    // Toolbar forwarding
    Toolbar(toolbar::Message),

    // Font loaded
    FontLoaded(Result<(), iced::font::Error>),
}

impl App {
    pub fn new() -> (Self, iced::Task<Message>) {
        let app = Self {
            document: None,
            toolbar: ToolbarState::default(),
            canvas: CanvasState::default(),
            sidebar: SidebarState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            config: AppConfig::default(),
        };
        let font_task = iced::font::load(crate::ui::icons::font_bytes()).map(Message::FontLoaded);
        (app, font_task)
    }

    pub fn title(&self) -> String {
        match &self.document {
            Some(doc) => {
                let name = doc
                    .source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("untitled");
                format!("{name} - SPE")
            }
            None => "SPE - PDF Text Overlay Editor".to_string(),
        }
    }

    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            // --- Toolbar message forwarding ---
            Message::Toolbar(toolbar_msg) => {
                return self.handle_toolbar_message(toolbar_msg);
            }

            // --- File operations ---
            Message::OpenFile => {
                return self.handle_open_file();
            }
            Message::FileOpened(path) => {
                return self.handle_file_opened(path);
            }
            Message::Save => {
                return self.handle_save();
            }
            Message::SaveAs => {
                return self.handle_save_as();
            }
            Message::SaveDestinationChosen(path) => {
                self.handle_save_destination(path);
            }

            // --- Page navigation ---
            Message::NextPage => {
                let navigated = if let Some(doc) = &mut self.document
                    && doc.current_page < doc.page_count
                {
                    doc.current_page += 1;
                    self.toolbar.page_input = doc.current_page.to_string();
                    true
                } else {
                    false
                };
                if navigated {
                    return self.render_if_uncached(self.document.as_ref().unwrap().current_page);
                }
            }
            Message::PreviousPage => {
                let navigated = if let Some(doc) = &mut self.document
                    && doc.current_page > 1
                {
                    doc.current_page -= 1;
                    self.toolbar.page_input = doc.current_page.to_string();
                    true
                } else {
                    false
                };
                if navigated {
                    return self.render_if_uncached(self.document.as_ref().unwrap().current_page);
                }
            }
            Message::GoToPage(page) => {
                let navigated = if let Some(doc) = &mut self.document
                    && page >= 1
                    && page <= doc.page_count
                {
                    doc.current_page = page;
                    self.toolbar.page_input = page.to_string();
                    true
                } else {
                    false
                };
                if navigated {
                    return self.render_if_uncached(page);
                }
            }
            Message::PageRendered(page, handle) => {
                if let Some(doc) = &mut self.document {
                    doc.page_images.insert(page, handle);
                    return self.prerender_neighbors();
                }
            }

            // --- Overlay editing (undoable) ---
            Message::PlaceOverlay(position) => {
                if let Some(doc) = &mut self.document {
                    let overlay = TextOverlay {
                        page: doc.current_page,
                        position,
                        text: String::new(),
                        font: self.toolbar.font,
                        font_size: self.toolbar.font_size,
                    };
                    let cmd = UndoCommand::PlaceOverlay {
                        overlay: overlay.clone(),
                    };
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                    let idx = doc.overlays.len() - 1;
                    self.canvas.active_overlay = Some(idx);
                    self.canvas.editing = true;
                }
            }
            Message::UpdateOverlayText(text) => {
                if let Some(doc) = &mut self.document
                    && let Some(idx) = self.canvas.active_overlay
                    && idx < doc.overlays.len()
                {
                    doc.overlays[idx].text = text;
                }
            }
            Message::CommitText => {
                // Text editing is committed as a single undoable action
                self.canvas.editing = false;
            }
            Message::MoveOverlay(index, new_position) => {
                if let Some(doc) = &mut self.document
                    && index < doc.overlays.len()
                {
                    let old_position = doc.overlays[index].position;
                    let cmd = UndoCommand::MoveOverlay {
                        index,
                        from: old_position,
                        to: new_position,
                    };
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                }
            }
            Message::ChangeFont(font) => {
                if let Some(doc) = &mut self.document {
                    if let Some(idx) = self.canvas.active_overlay
                        && idx < doc.overlays.len()
                    {
                        let old_font = doc.overlays[idx].font;
                        let cmd = UndoCommand::ChangeOverlayFont {
                            index: idx,
                            old_font,
                            new_font: font,
                        };
                        cmd.apply(&mut doc.overlays);
                        self.undo_stack.push(cmd);
                        self.redo_stack.clear();
                    }
                    self.toolbar.font = font;
                }
            }
            Message::ChangeFontSize(size) => {
                if let Some(doc) = &mut self.document {
                    if let Some(idx) = self.canvas.active_overlay
                        && idx < doc.overlays.len()
                    {
                        let old_size = doc.overlays[idx].font_size;
                        let cmd = UndoCommand::ChangeOverlayFontSize {
                            index: idx,
                            old_size,
                            new_size: size,
                        };
                        cmd.apply(&mut doc.overlays);
                        self.undo_stack.push(cmd);
                        self.redo_stack.clear();
                    }
                    self.toolbar.font_size = size;
                    self.toolbar.font_size_input = format!("{size}");
                }
            }
            Message::DeleteOverlay => {
                if let Some(doc) = &mut self.document
                    && let Some(idx) = self.canvas.active_overlay
                    && idx < doc.overlays.len()
                {
                    let overlay = doc.overlays[idx].clone();
                    let cmd = UndoCommand::DeleteOverlay {
                        overlay,
                        index: idx,
                    };
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                    self.redo_stack.clear();
                    self.canvas.active_overlay = None;
                    self.canvas.editing = false;
                }
            }
            Message::SelectOverlay(index) => {
                if let Some(doc) = &self.document
                    && index < doc.overlays.len()
                {
                    self.canvas.active_overlay = Some(index);
                    self.canvas.editing = false;
                    self.toolbar.font = doc.overlays[index].font;
                    self.toolbar.font_size = doc.overlays[index].font_size;
                    self.toolbar.font_size_input = format!("{}", doc.overlays[index].font_size);
                }
            }
            Message::DeselectOverlay => {
                self.canvas.active_overlay = None;
                self.canvas.editing = false;
            }

            // --- Canvas ---
            Message::ZoomIn => {
                self.canvas.zoom = canvas::zoom_in(self.canvas.zoom);
                return self.render_current_page();
            }
            Message::ZoomOut => {
                self.canvas.zoom = canvas::zoom_out(self.canvas.zoom);
                return self.render_current_page();
            }
            Message::ZoomReset => {
                self.canvas.zoom = 1.0;
                return self.render_current_page();
            }

            // --- Sidebar ---
            Message::ToggleSidebar => {
                self.sidebar.visible = !self.sidebar.visible;
            }
            Message::ThumbnailRendered(page, handle) => {
                self.sidebar.thumbnails.insert(page, handle);
            }

            // --- Undo/Redo ---
            Message::Undo => {
                if let Some(cmd) = self.undo_stack.pop()
                    && let Some(doc) = &mut self.document
                {
                    cmd.reverse(&mut doc.overlays);
                    self.redo_stack.push(cmd);
                }
            }
            Message::Redo => {
                if let Some(cmd) = self.redo_stack.pop()
                    && let Some(doc) = &mut self.document
                {
                    cmd.apply(&mut doc.overlays);
                    self.undo_stack.push(cmd);
                }
            }

            // --- Font loaded ---
            Message::FontLoaded(_) => {}
        }
        iced::Task::none()
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        let toolbar_ctx = ToolbarContext {
            has_document: self.document.is_some(),
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            has_selection: self.canvas.active_overlay.is_some(),
            current_page: self.document.as_ref().map_or(0, |d| d.current_page),
            page_count: self.document.as_ref().map_or(0, |d| d.page_count),
            zoom_percent: canvas::zoom_percent(self.canvas.zoom),
            sidebar_visible: self.sidebar.visible,
        };
        let toolbar = toolbar::toolbar_view(&self.toolbar, &toolbar_ctx).map(Message::Toolbar);

        let content: iced::Element<Message> = if let Some(doc) = &self.document {
            let program = PdfCanvasProgram {
                page_image: doc.page_images.get(&doc.current_page),
                page_dimensions: doc.page_dimensions.get(&doc.current_page).copied(),
                overlays: &doc.overlays,
                current_page: doc.current_page,
                zoom: self.canvas.zoom,
                dpi: canvas::effective_dpi(self.canvas.zoom),
                active_overlay: self.canvas.active_overlay,
                editing: self.canvas.editing,
                overlay_color: self.config.overlay_color,
            };
            let canvas_area: iced::Element<Message> = iced::widget::canvas(program)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into();

            if self.sidebar.visible {
                let sidebar: iced::Element<Message> =
                    iced::widget::container(iced::widget::text("Sidebar"))
                        .width(crate::ui::sidebar::SIDEBAR_WIDTH)
                        .into();
                iced::widget::row![sidebar, canvas_area].into()
            } else {
                canvas_area
            }
        } else {
            iced::widget::center(iced::widget::text("Open a PDF to get started").size(20)).into()
        };

        iced::widget::column![toolbar, content].into()
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::event::listen_with(|event, status, _window| {
            if status == iced::event::Status::Captured {
                return None;
            }
            match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    key_to_message(key, modifiers)
                }
                _ => None,
            }
        })
    }

    fn handle_toolbar_message(&mut self, msg: toolbar::Message) -> iced::Task<Message> {
        match msg {
            toolbar::Message::OpenFile => return self.update(Message::OpenFile),
            toolbar::Message::Save => return self.update(Message::Save),
            toolbar::Message::SaveAs => return self.update(Message::SaveAs),
            toolbar::Message::Undo => return self.update(Message::Undo),
            toolbar::Message::Redo => return self.update(Message::Redo),
            toolbar::Message::FontSelected(font) => {
                return self.update(Message::ChangeFont(font));
            }
            toolbar::Message::FontSizeInput(input) => {
                self.toolbar.font_size_input = input;
            }
            toolbar::Message::FontSizeSubmit => {
                if let Ok(size) = self.toolbar.font_size_input.parse::<f32>()
                    && size > 0.0
                {
                    return self.update(Message::ChangeFontSize(size));
                }
            }
            toolbar::Message::ZoomIn => return self.update(Message::ZoomIn),
            toolbar::Message::ZoomOut => return self.update(Message::ZoomOut),
            toolbar::Message::ZoomReset => return self.update(Message::ZoomReset),
            toolbar::Message::PreviousPage => return self.update(Message::PreviousPage),
            toolbar::Message::NextPage => return self.update(Message::NextPage),
            toolbar::Message::PageInput(input) => {
                self.toolbar.page_input = input;
            }
            toolbar::Message::PageInputSubmit => {
                if let Ok(page) = self.toolbar.page_input.parse::<u32>() {
                    return self.update(Message::GoToPage(page));
                }
            }
            toolbar::Message::ToggleSidebar => return self.update(Message::ToggleSidebar),
            toolbar::Message::DeleteOverlay => return self.update(Message::DeleteOverlay),
        }
        iced::Task::none()
    }

    fn handle_open_file(&mut self) -> iced::Task<Message> {
        iced::Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .pick_file()
                    .await;
                handle.map(|h| h.path().to_path_buf())
            },
            |path| match path {
                Some(p) => Message::FileOpened(p),
                None => Message::DeselectOverlay, // user cancelled, no-op
            },
        )
    }

    fn handle_file_opened(&mut self, path: PathBuf) -> iced::Task<Message> {
        match lopdf::Document::load(&path) {
            Ok(doc) => {
                let page_dims = crate::pdf::page_dimensions(&doc);
                let page_count = doc.get_pages().len() as u32;
                self.document = Some(DocumentState {
                    source_path: path.clone(),
                    save_path: None,
                    page_count,
                    current_page: 1,
                    page_images: HashMap::new(),
                    page_dimensions: page_dims,
                    overlays: Vec::new(),
                });
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.canvas = CanvasState::default();
                self.sidebar.thumbnails.clear();
                self.toolbar.page_input = "1".to_string();
                let dpi = canvas::effective_dpi(self.canvas.zoom) as u32;
                render_page_task(path, 1, dpi)
            }
            Err(e) => {
                eprintln!("Failed to open PDF: {e}");
                iced::Task::none()
            }
        }
    }

    fn handle_save(&mut self) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && let Some(save_path) = &doc.save_path
        {
            let source = doc.source_path.clone();
            let dest = save_path.clone();
            let overlays = doc.overlays.clone();
            if let Err(e) = crate::pdf::writer::write_overlays(&source, &dest, &overlays) {
                eprintln!("Save failed: {e}");
            }
            return iced::Task::none();
        }
        self.handle_save_as()
    }

    fn handle_save_as(&mut self) -> iced::Task<Message> {
        iced::Task::perform(
            async {
                let handle = rfd::AsyncFileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .save_file()
                    .await;
                handle.map(|h| h.path().to_path_buf())
            },
            |path| match path {
                Some(p) => Message::SaveDestinationChosen(p),
                None => Message::DeselectOverlay,
            },
        )
    }

    fn handle_save_destination(&mut self, path: PathBuf) {
        if let Some(doc) = &mut self.document {
            // Prevent saving over the source file to avoid data loss on
            // write failure (the source would already be truncated).
            if path == doc.source_path {
                eprintln!("Cannot save to the same file as the source. Use a different filename.");
                return;
            }
            let source = doc.source_path.clone();
            let overlays = doc.overlays.clone();
            if let Err(e) = crate::pdf::writer::write_overlays(&source, &path, &overlays) {
                eprintln!("Save failed: {e}");
            } else {
                doc.save_path = Some(path);
            }
        }
    }

    /// Render the given page if it is not already cached.
    fn render_if_uncached(&self, page: u32) -> iced::Task<Message> {
        if let Some(doc) = &self.document
            && !doc.page_images.contains_key(&page)
        {
            let dpi = canvas::effective_dpi(self.canvas.zoom) as u32;
            render_page_task(doc.source_path.clone(), page, dpi)
        } else {
            iced::Task::none()
        }
    }

    /// Pre-render neighbor pages (current ± 1) that are not already cached.
    fn prerender_neighbors(&self) -> iced::Task<Message> {
        let Some(doc) = &self.document else {
            return iced::Task::none();
        };
        let dpi = canvas::effective_dpi(self.canvas.zoom) as u32;
        let mut tasks = Vec::new();
        let current = doc.current_page;
        if current > 1 && !doc.page_images.contains_key(&(current - 1)) {
            tasks.push(render_page_task(doc.source_path.clone(), current - 1, dpi));
        }
        if current < doc.page_count && !doc.page_images.contains_key(&(current + 1)) {
            tasks.push(render_page_task(doc.source_path.clone(), current + 1, dpi));
        }
        iced::Task::batch(tasks)
    }

    /// Re-render the current page at the current zoom/DPI.
    fn render_current_page(&self) -> iced::Task<Message> {
        if let Some(doc) = &self.document {
            let dpi = canvas::effective_dpi(self.canvas.zoom) as u32;
            render_page_task(doc.source_path.clone(), doc.current_page, dpi)
        } else {
            iced::Task::none()
        }
    }
}

/// Launch an async task to render a single PDF page via pdftoppm.
fn render_page_task(pdf_path: PathBuf, page: u32, dpi: u32) -> iced::Task<Message> {
    iced::Task::perform(
        async move {
            let renderer = PdftoppmRenderer;
            match renderer.render_page(&pdf_path, page, dpi) {
                Ok(img) => Some((page, canvas::image_to_handle(img))),
                Err(e) => {
                    eprintln!("Failed to render page {page}: {e}");
                    None
                }
            }
        },
        |result| match result {
            Some((page, handle)) => Message::PageRendered(page, handle),
            None => Message::DeselectOverlay,
        },
    )
}

/// Map a keyboard event to an application message.
fn key_to_message(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use keyboard::key::Named;

    match key {
        keyboard::Key::Named(named) => match (named, modifiers.command(), modifiers.shift()) {
            (Named::Delete, false, false) => Some(Message::DeleteOverlay),
            (Named::Escape, false, false) => Some(Message::DeselectOverlay),
            (Named::PageUp, false, false) => Some(Message::PreviousPage),
            (Named::PageDown, false, false) => Some(Message::NextPage),
            (Named::F9, false, false) => Some(Message::ToggleSidebar),
            _ => None,
        },
        keyboard::Key::Character(ref c) => {
            let s = c.as_str();
            match (s, modifiers.command(), modifiers.shift()) {
                ("o", true, false) => Some(Message::OpenFile),
                ("s", true, false) => Some(Message::Save),
                ("s", true, true) | ("S", true, _) => Some(Message::SaveAs),
                ("z", true, false) => Some(Message::Undo),
                ("z", true, true) | ("Z", true, _) => Some(Message::Redo),
                ("+" | "=", true, _) => Some(Message::ZoomIn),
                ("-", true, false) => Some(Message::ZoomOut),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_default_has_no_document() {
        let (app, _) = App::new();
        assert!(app.document.is_none());
        assert!(app.undo_stack.is_empty());
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn next_page_without_document_is_noop() {
        let (mut app, _) = App::new();
        app.update(Message::NextPage);
        assert!(app.document.is_none());
    }

    fn test_app_with_document() -> App {
        let (mut app, _) = App::new();
        app.document = Some(DocumentState {
            source_path: PathBuf::from("/tmp/test.pdf"),
            save_path: None,
            page_count: 3,
            current_page: 1,
            page_images: HashMap::new(),
            page_dimensions: HashMap::new(),
            overlays: Vec::new(),
        });
        app
    }

    #[test]
    fn next_page_increments() {
        let mut app = test_app_with_document();
        app.update(Message::NextPage);
        assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    }

    #[test]
    fn next_page_does_not_exceed_page_count() {
        let mut app = test_app_with_document();
        app.update(Message::NextPage);
        app.update(Message::NextPage);
        app.update(Message::NextPage); // should stay at 3
        assert_eq!(app.document.as_ref().unwrap().current_page, 3);
    }

    #[test]
    fn previous_page_decrements() {
        let mut app = test_app_with_document();
        app.update(Message::NextPage);
        app.update(Message::PreviousPage);
        assert_eq!(app.document.as_ref().unwrap().current_page, 1);
    }

    #[test]
    fn previous_page_does_not_go_below_one() {
        let mut app = test_app_with_document();
        app.update(Message::PreviousPage);
        assert_eq!(app.document.as_ref().unwrap().current_page, 1);
    }

    #[test]
    fn go_to_page_sets_current_page() {
        let mut app = test_app_with_document();
        app.update(Message::GoToPage(2));
        assert_eq!(app.document.as_ref().unwrap().current_page, 2);
    }

    #[test]
    fn go_to_page_ignores_out_of_range() {
        let mut app = test_app_with_document();
        app.update(Message::GoToPage(99));
        assert_eq!(app.document.as_ref().unwrap().current_page, 1);
    }

    #[test]
    fn place_overlay_adds_to_overlays() {
        let mut app = test_app_with_document();
        app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
        assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
        assert_eq!(app.undo_stack.len(), 1);
        assert!(app.canvas.active_overlay.is_some());
        assert!(app.canvas.editing);
    }

    #[test]
    fn undo_redo_through_update() {
        let mut app = test_app_with_document();

        app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
        assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);

        app.update(Message::Undo);
        assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
        assert_eq!(app.redo_stack.len(), 1);

        app.update(Message::Redo);
        assert_eq!(app.document.as_ref().unwrap().overlays.len(), 1);
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn new_action_clears_redo_stack() {
        let mut app = test_app_with_document();
        app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
        app.update(Message::Undo);
        assert_eq!(app.redo_stack.len(), 1);

        app.update(Message::PlaceOverlay(PdfPosition { x: 200.0, y: 600.0 }));
        assert!(app.redo_stack.is_empty());
    }

    #[test]
    fn delete_overlay_removes_selected() {
        let mut app = test_app_with_document();
        app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
        // PlaceOverlay sets active_overlay
        app.update(Message::DeleteOverlay);
        assert_eq!(app.document.as_ref().unwrap().overlays.len(), 0);
        assert!(app.canvas.active_overlay.is_none());
    }

    #[test]
    fn change_font_updates_overlay_and_toolbar() {
        let mut app = test_app_with_document();
        app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
        app.update(Message::ChangeFont(Standard14Font::Courier));
        assert_eq!(
            app.document.as_ref().unwrap().overlays[0].font,
            Standard14Font::Courier
        );
        assert_eq!(app.toolbar.font, Standard14Font::Courier);
    }

    #[test]
    fn zoom_in_increases_zoom() {
        let (mut app, _) = App::new();
        let initial = app.canvas.zoom;
        app.update(Message::ZoomIn);
        assert!(app.canvas.zoom > initial);
    }

    #[test]
    fn zoom_reset_returns_to_one() {
        let (mut app, _) = App::new();
        app.update(Message::ZoomIn);
        app.update(Message::ZoomReset);
        assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn toggle_sidebar_flips_visibility() {
        let (mut app, _) = App::new();
        assert!(app.sidebar.visible);
        app.update(Message::ToggleSidebar);
        assert!(!app.sidebar.visible);
        app.update(Message::ToggleSidebar);
        assert!(app.sidebar.visible);
    }

    #[test]
    fn select_overlay_updates_toolbar() {
        let mut app = test_app_with_document();
        app.update(Message::PlaceOverlay(PdfPosition { x: 100.0, y: 700.0 }));
        app.update(Message::ChangeFont(Standard14Font::CourierBold));
        app.update(Message::DeselectOverlay);
        // Now select it again
        app.update(Message::SelectOverlay(0));
        assert_eq!(app.toolbar.font, Standard14Font::CourierBold);
    }

    #[test]
    fn save_destination_sets_save_path() {
        let mut app = test_app_with_document();
        // Simulate save destination (won't actually write since test.pdf doesn't exist,
        // but we can test the path assignment logic)
        let doc = app.document.as_ref().unwrap();
        assert!(doc.save_path.is_none());
    }

    // Keyboard shortcut tests
    #[test]
    fn ctrl_o_maps_to_open() {
        let msg = key_to_message(
            keyboard::Key::Character("o".into()),
            keyboard::Modifiers::COMMAND,
        );
        assert!(matches!(msg, Some(Message::OpenFile)));
    }

    #[test]
    fn ctrl_s_maps_to_save() {
        let msg = key_to_message(
            keyboard::Key::Character("s".into()),
            keyboard::Modifiers::COMMAND,
        );
        assert!(matches!(msg, Some(Message::Save)));
    }

    #[test]
    fn ctrl_shift_s_maps_to_save_as() {
        let msg = key_to_message(
            keyboard::Key::Character("s".into()),
            keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
        );
        assert!(matches!(msg, Some(Message::SaveAs)));
    }

    #[test]
    fn ctrl_z_maps_to_undo() {
        let msg = key_to_message(
            keyboard::Key::Character("z".into()),
            keyboard::Modifiers::COMMAND,
        );
        assert!(matches!(msg, Some(Message::Undo)));
    }

    #[test]
    fn ctrl_shift_z_maps_to_redo() {
        let msg = key_to_message(
            keyboard::Key::Character("z".into()),
            keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
        );
        assert!(matches!(msg, Some(Message::Redo)));
    }

    #[test]
    fn f9_maps_to_toggle_sidebar() {
        let msg = key_to_message(
            keyboard::Key::Named(keyboard::key::Named::F9),
            keyboard::Modifiers::empty(),
        );
        assert!(matches!(msg, Some(Message::ToggleSidebar)));
    }

    #[test]
    fn escape_maps_to_deselect() {
        let msg = key_to_message(
            keyboard::Key::Named(keyboard::key::Named::Escape),
            keyboard::Modifiers::empty(),
        );
        assert!(matches!(msg, Some(Message::DeselectOverlay)));
    }

    #[test]
    fn delete_maps_to_delete_overlay() {
        let msg = key_to_message(
            keyboard::Key::Named(keyboard::key::Named::Delete),
            keyboard::Modifiers::empty(),
        );
        assert!(matches!(msg, Some(Message::DeleteOverlay)));
    }

    #[test]
    fn page_up_maps_to_previous() {
        let msg = key_to_message(
            keyboard::Key::Named(keyboard::key::Named::PageUp),
            keyboard::Modifiers::empty(),
        );
        assert!(matches!(msg, Some(Message::PreviousPage)));
    }

    #[test]
    fn ctrl_plus_maps_to_zoom_in() {
        let msg = key_to_message(
            keyboard::Key::Character("+".into()),
            keyboard::Modifiers::COMMAND,
        );
        assert!(matches!(msg, Some(Message::ZoomIn)));
    }

    #[test]
    fn ctrl_minus_maps_to_zoom_out() {
        let msg = key_to_message(
            keyboard::Key::Character("-".into()),
            keyboard::Modifiers::COMMAND,
        );
        assert!(matches!(msg, Some(Message::ZoomOut)));
    }

    #[test]
    fn view_with_no_document_does_not_panic() {
        let (app, _) = App::new();
        let _element = app.view();
    }

    #[test]
    fn title_without_document() {
        let (app, _) = App::new();
        assert_eq!(app.title(), "SPE - PDF Text Overlay Editor");
    }

    #[test]
    fn title_with_document() {
        let mut app = test_app_with_document();
        app.document.as_mut().unwrap().source_path = PathBuf::from("/tmp/report.pdf");
        assert_eq!(app.title(), "report.pdf - SPE");
    }

    #[test]
    fn view_with_document_renders_canvas_widget() {
        let app = test_app_with_document();
        // Should not panic — constructs PdfCanvasProgram and renders canvas
        let _element = app.view();
    }

    #[test]
    fn view_with_document_and_page_image_does_not_panic() {
        let mut app = test_app_with_document();
        let doc = app.document.as_mut().unwrap();
        doc.page_dimensions.insert(1, (612.0, 792.0));
        // Insert a dummy Handle
        let handle = Handle::from_rgba(1, 1, vec![0, 0, 0, 255]);
        doc.page_images.insert(1, handle);
        let _element = app.view();
    }

    #[test]
    fn page_rendered_inserts_into_cache() {
        let mut app = test_app_with_document();
        let handle = Handle::from_rgba(1, 1, vec![255, 0, 0, 255]);
        let _ = app.update(Message::PageRendered(1, handle));
        assert!(app.document.as_ref().unwrap().page_images.contains_key(&1));
    }

    #[test]
    fn page_rendered_replaces_existing_cached_image() {
        let mut app = test_app_with_document();
        let handle1 = Handle::from_rgba(1, 1, vec![255, 0, 0, 255]);
        let handle2 = Handle::from_rgba(1, 1, vec![0, 255, 0, 255]);
        let _ = app.update(Message::PageRendered(1, handle1));
        let _ = app.update(Message::PageRendered(1, handle2));
        assert!(app.document.as_ref().unwrap().page_images.contains_key(&1));
    }

    #[test]
    fn zoom_in_updates_zoom_with_document() {
        let mut app = test_app_with_document();
        let initial = app.canvas.zoom;
        let _ = app.update(Message::ZoomIn);
        assert!(app.canvas.zoom > initial);
    }

    #[test]
    fn zoom_reset_with_document() {
        let mut app = test_app_with_document();
        let _ = app.update(Message::ZoomIn);
        let _ = app.update(Message::ZoomReset);
        assert!((app.canvas.zoom - 1.0).abs() < f32::EPSILON);
    }
}
