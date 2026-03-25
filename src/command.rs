// Undo/redo command model: each variant captures enough state to apply and reverse the operation.

use crate::fonts::FontId;
use crate::overlay::{PdfPosition, TextOverlay};

/// A reversible editing operation on the overlay list.
#[derive(Debug, Clone)]
pub enum Command {
    PlaceOverlay {
        overlay: TextOverlay,
    },
    DeleteOverlay {
        overlay: TextOverlay,
        index: usize,
    },
    MoveOverlay {
        index: usize,
        from: PdfPosition,
        to: PdfPosition,
    },
    EditText {
        index: usize,
        old_text: String,
        new_text: String,
    },
    ChangeOverlayFont {
        index: usize,
        old_font: FontId,
        new_font: FontId,
    },
    ChangeOverlayFontSize {
        index: usize,
        old_size: f32,
        new_size: f32,
    },
    ResizeOverlay {
        index: usize,
        old_width: f32,
        new_width: f32,
    },
}

impl Command {
    /// Applies this command to the overlay list.
    pub fn apply(&self, overlays: &mut Vec<TextOverlay>) {
        match self {
            Self::PlaceOverlay { overlay } => overlays.push(overlay.clone()),
            Self::DeleteOverlay { index, .. } => {
                overlays.remove(*index);
            }
            Self::MoveOverlay { index, to, .. } => overlays[*index].position = *to,
            Self::EditText {
                index, new_text, ..
            } => overlays[*index].text = new_text.clone(),
            Self::ChangeOverlayFont {
                index, new_font, ..
            } => overlays[*index].font = *new_font,
            Self::ChangeOverlayFontSize {
                index, new_size, ..
            } => {
                overlays[*index].font_size = *new_size;
            }
            Self::ResizeOverlay {
                index, new_width, ..
            } => {
                overlays[*index].width = Some(*new_width);
            }
        }
    }

    /// Reverses this command, restoring the overlay list to its prior state.
    pub fn reverse(&self, overlays: &mut Vec<TextOverlay>) {
        match self {
            Self::PlaceOverlay { .. } => {
                overlays.pop();
            }
            Self::DeleteOverlay { overlay, index } => overlays.insert(*index, overlay.clone()),
            Self::MoveOverlay { index, from, .. } => overlays[*index].position = *from,
            Self::EditText {
                index, old_text, ..
            } => overlays[*index].text = old_text.clone(),
            Self::ChangeOverlayFont {
                index, old_font, ..
            } => overlays[*index].font = *old_font,
            Self::ChangeOverlayFontSize {
                index, old_size, ..
            } => {
                overlays[*index].font_size = *old_size;
            }
            Self::ResizeOverlay {
                index, old_width, ..
            } => {
                overlays[*index].width = Some(*old_width);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::FontRegistry;

    fn sample_overlay() -> TextOverlay {
        let registry = FontRegistry::new();
        TextOverlay {
            page: 1,
            position: PdfPosition { x: 72.0, y: 720.0 },
            text: "Hello".to_string(),
            font: registry.default_font(),
            font_size: 12.0,
            width: None,
        }
    }

    #[test]
    fn place_overlay_apply_adds_to_vec() {
        let mut overlays = vec![];
        let cmd = Command::PlaceOverlay {
            overlay: sample_overlay(),
        };
        cmd.apply(&mut overlays);
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].text, "Hello");
    }

    #[test]
    fn place_overlay_reverse_removes_from_vec() {
        let mut overlays = vec![sample_overlay()];
        let cmd = Command::PlaceOverlay {
            overlay: sample_overlay(),
        };
        cmd.reverse(&mut overlays);
        assert!(overlays.is_empty());
    }

    #[test]
    fn delete_overlay_round_trip() {
        let overlay = sample_overlay();
        let mut overlays = vec![overlay.clone()];
        let cmd = Command::DeleteOverlay {
            overlay: overlay.clone(),
            index: 0,
        };
        cmd.apply(&mut overlays);
        assert!(overlays.is_empty());
        cmd.reverse(&mut overlays);
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0].text, "Hello");
    }

    #[test]
    fn move_overlay_round_trip() {
        let mut overlays = vec![sample_overlay()];
        let from = PdfPosition { x: 72.0, y: 720.0 };
        let to = PdfPosition { x: 200.0, y: 500.0 };
        let cmd = Command::MoveOverlay { index: 0, from, to };
        cmd.apply(&mut overlays);
        assert!((overlays[0].position.x - 200.0).abs() < f32::EPSILON);
        assert!((overlays[0].position.y - 500.0).abs() < f32::EPSILON);
        cmd.reverse(&mut overlays);
        assert!((overlays[0].position.x - 72.0).abs() < f32::EPSILON);
        assert!((overlays[0].position.y - 720.0).abs() < f32::EPSILON);
    }

    #[test]
    fn edit_text_round_trip() {
        let mut overlays = vec![sample_overlay()];
        let cmd = Command::EditText {
            index: 0,
            old_text: "Hello".to_string(),
            new_text: "World".to_string(),
        };
        cmd.apply(&mut overlays);
        assert_eq!(overlays[0].text, "World");
        cmd.reverse(&mut overlays);
        assert_eq!(overlays[0].text, "Hello");
    }

    #[test]
    fn change_font_round_trip() {
        let registry = FontRegistry::new();
        let helvetica = registry.default_font();
        let courier = registry.find_by_name("Courier").unwrap();
        let mut overlays = vec![sample_overlay()];
        let cmd = Command::ChangeOverlayFont {
            index: 0,
            old_font: helvetica,
            new_font: courier,
        };
        cmd.apply(&mut overlays);
        assert_eq!(overlays[0].font, courier);
        cmd.reverse(&mut overlays);
        assert_eq!(overlays[0].font, helvetica);
    }

    #[test]
    fn change_font_size_round_trip() {
        let mut overlays = vec![sample_overlay()];
        let cmd = Command::ChangeOverlayFontSize {
            index: 0,
            old_size: 12.0,
            new_size: 24.0,
        };
        cmd.apply(&mut overlays);
        assert!((overlays[0].font_size - 24.0).abs() < f32::EPSILON);
        cmd.reverse(&mut overlays);
        assert!((overlays[0].font_size - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn full_undo_redo_cycle() {
        let mut overlays = vec![];
        let mut undo_stack: Vec<Command> = vec![];
        let mut redo_stack: Vec<Command> = vec![];

        // Place overlay
        let cmd = Command::PlaceOverlay {
            overlay: sample_overlay(),
        };
        cmd.apply(&mut overlays);
        undo_stack.push(cmd);
        redo_stack.clear();
        assert_eq!(overlays.len(), 1);

        // Undo
        let cmd = undo_stack.pop().unwrap();
        cmd.reverse(&mut overlays);
        redo_stack.push(cmd);
        assert!(overlays.is_empty());

        // Redo
        let cmd = redo_stack.pop().unwrap();
        cmd.apply(&mut overlays);
        undo_stack.push(cmd);
        assert_eq!(overlays.len(), 1);
    }

    #[test]
    fn delete_at_middle_index_restores_correctly() {
        let registry = FontRegistry::new();
        let courier = registry.find_by_name("Courier").unwrap();
        let times = registry.find_by_name("Times Roman").unwrap();
        let mut overlays = vec![
            sample_overlay(),
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 100.0, y: 600.0 },
                text: "Second".to_string(),
                font: courier,
                font_size: 14.0,
                width: None,
            },
            TextOverlay {
                page: 1,
                position: PdfPosition { x: 200.0, y: 500.0 },
                text: "Third".to_string(),
                font: times,
                font_size: 16.0,
                width: None,
            },
        ];
        let deleted = overlays[1].clone();
        let cmd = Command::DeleteOverlay {
            overlay: deleted,
            index: 1,
        };
        cmd.apply(&mut overlays);
        assert_eq!(overlays.len(), 2);
        assert_eq!(overlays[0].text, "Hello");
        assert_eq!(overlays[1].text, "Third");
        cmd.reverse(&mut overlays);
        assert_eq!(overlays.len(), 3);
        assert_eq!(overlays[1].text, "Second");
    }

    #[test]
    fn resize_overlay_round_trip() {
        let mut overlay = sample_overlay();
        overlay.width = Some(200.0);
        let mut overlays = vec![overlay];
        let cmd = Command::ResizeOverlay {
            index: 0,
            old_width: 200.0,
            new_width: 300.0,
        };
        cmd.apply(&mut overlays);
        assert!((overlays[0].width.unwrap() - 300.0).abs() < f32::EPSILON);
        cmd.reverse(&mut overlays);
        assert!((overlays[0].width.unwrap() - 200.0).abs() < f32::EPSILON);
    }
}
