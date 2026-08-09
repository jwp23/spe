# ADR-012: Undo and Edit-Session Semantics

## Context

Placing or editing an overlay opens an edit session: the user types, picks a font, resizes the box, or moves it, all before the overlay is committed to the document's undo history. Several bugs (spe-6v8, spe-164) traced to that gap between live editing state and the recorded history:

- Redo could resurrect a text-less "ghost" overlay after an undo had removed it mid-edit.
- Selecting or editing a different overlay while a session was open could discard the wrong overlay, or leave the document diverged from what the undo stack could reconstruct.
- The undo stack records overlays by raw `Vec` index (`command.rs`), so an operation that inserts or removes an overlay elsewhere in the list can leave an in-flight session, or another recorded command, addressing the wrong slot.

Root-causing spe-6v8/spe-164 (bd notes) found the indices themselves were not the problem: replaying the undo stack against the live document under randomized command sequences (a property test seeded 1..2000 in-suite, verified separately at 200,000 seeds) never found a case where LIFO undo/redo discipline let a raw index drift from the overlay it was meant to address. The actual defect was edit-session state (`editing`, `active_overlay`, `edit_start_text`, `fresh_placement`) surviving operations that invalidate it.

A further UX gap (spe-5d1) showed the resulting model was coarser than users expected: undo while an edit box was open cancelled the whole in-progress overlay, discarding a font or size change along with everything else, rather than stepping back one change at a time the way a text editor does.

## Decision

**Uncommitted session state never enters the document history.** The document `undo_stack`/`redo_stack` hold only `Command` values (`command.rs`) that describe committed, reversible changes to the overlay list. Typing, in-progress font/size picks, and drag previews live in canvas state (`editing`, `active_overlay`, `edit_start_text`, `fresh_placement`) until the session ends.

**Every action that retargets or removes overlays commits the pending edit first.** `handle_select_overlay`, `handle_edit_overlay`, `handle_deselect_overlay`, and undo/redo all route through `commit_before_targeting` or an equivalent commit/cancel step (`src/app/handlers.rs`) before acting on a different index. This closes the gap where a stale session could discard the wrong overlay or leave the document ahead of its history.

**Raw indices are safe under LIFO discipline.** `Command` addresses overlays by `usize` index with no stable ID and no index-adjustment on insert/remove. This is deliberately unguarded: because commands only ever undo/redo in strict last-in-first-out order, and every count-changing command (`PlaceOverlay`, `DeleteOverlay`) is always the most recently pushed relative to anything it could invalidate, no other recorded command can end up pointing past a boundary a LIFO pop hasn't already crossed. This was verified, not assumed: the property test in `src/app/tests.rs` replays randomized interleavings of place/edit/select/undo/redo/resize against the live document and asserts the two never diverge, at 2,000 seeds in the committed suite and 200,000 seeds run separately as fuzz evidence.

Two alternatives were rejected in favor of raw indices — see Trade-offs.

**Selection survives undo/redo iff the popped command preserves overlay count.** `Command::changes_overlay_count()` returns true only for `PlaceOverlay`/`DeleteOverlay`. Undoing or redoing an in-place command (text, font, size, move, resize) leaves every index addressing the same overlay, so the active selection and toolbar state resync rather than clear; undoing/redoing a placement or deletion clears the edit session, since the indices it held may no longer mean the same thing.

**Undo is session-granular while a box is open, coarse once committed** (Joe's explicit ruling, spe-5d1). `SessionHistory` (`command.rs`) tracks steps taken inside the current session: `handle_undo` walks those first, then falls through to the document history only once the session has nothing left to step back through. This is a gradient, not two independent stacks:

- A run of typing coalesces into one step (`SessionHistory::record_text`); any other action ends the run, so the next keystroke starts a burst of its own.
- Closing the edit box (with nothing left in the session to undo) is itself the visible change for that keystroke — `handle_undo` returns there rather than falling through and reversing a document command in the same breath, which would undo something the user never touched.
- Once a session is committed or cancelled, `session_history.clear()` runs and every subsequent undo/redo operates purely on the coarse, one-command-per-action document history. There is no finer-grained undo for a change made outside an open session.

**In-session document-changing steps are markers, not duplicated commands.** A font, size, move, or resize made inside an open session is already recorded on the document `undo_stack` as a `Command` (so it survives the session ending); `SessionHistory::record_document()` pushes only a `SessionStep::Document` marker meaning "the newest command belongs to this session." Stepping back through that marker just calls `undo_document_command()`. A `debug_assert` in `undo_session_step` pins the invariant that a count-changing command is never recorded as a session step — those always commit the session first, by construction, so this can't happen.

## Trade-offs

**Chosen: raw `Vec` indices, LIFO-only history**
- No extra bookkeeping (no ID generation, no index map)
- Safety rests on an invariant (LIFO order) rather than being structurally impossible, but that invariant was verified with 200k-seed fuzzing rather than merely assumed
- Session state that outlives its validity window is the actual risk this model carries — addressed separately by the commit-before-targeting rule above, not by the indexing scheme

**Rejected: stable overlay IDs**
- Would make an overlay's identity survive reordering / index shifts without care
- Adds a layer of indirection (ID → index lookup) to every command apply/reverse, for a failure mode (index drift under LIFO) that testing showed doesn't occur
- Rejected as unnecessary complexity given the fuzz evidence

**Rejected: index-adjusting on insert/remove**
- Shifting every stored index when an overlay is inserted/removed elsewhere in the list is exactly the kind of bookkeeping raw LIFO discipline makes unnecessary
- Would still need the same commit-before-targeting discipline to keep session state in sync, so it wouldn't have prevented spe-6v8/spe-164 on its own

**Rejected: duplicating in-session document changes as separate session steps**
- Recording a font/size change twice — once as a `Command`, once as a session-local record — risks the two histories drifting out of order relative to each other
- The marker approach keeps one ordering across both histories, checked by a debug assertion
