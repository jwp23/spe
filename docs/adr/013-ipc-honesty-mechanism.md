# ADR-013: IPC Honesty Mechanism

## Context

The IPC layer (`src/ipc.rs`) exists so the screenshot/automation tool can drive the app headlessly: open a document, click, type, undo, save, and so on. Early versions of individual commands could report success while doing nothing — the classic case being the screenshot harness's socat default silently dropping a slow reply, which is indistinguishable, from the caller's side, from the app itself lying about what happened (PR #127).

`IpcCommand::to_message` translates a wire command into an `App::Message`, but building a `Message` successfully only proves the command *could* be dispatched — not that the resulting `update()` call actually did the requested thing. A command like `Type` sent with no overlay selected, `Undo` sent with an empty undo stack, or `Redo` sent while an edit session has nothing to redo would previously produce a `Message` that ran through `update()` and quietly no-op'd, while the IPC reply still said `ok: true`.

## Decision

Every IPC reply reflects whether the action actually happened, enforced through two mechanisms that between them cover the two kinds of failure a command can have:

**Precondition checks run before dispatch, against a borrowed, read-only context.** `CommandContext<'a>` (`src/ipc.rs`) is a snapshot of exactly the state a precondition needs: the document, active overlay, editing flag, and undo/redo/session-redo depths. `IpcCommand::to_message(&self, ctx: &CommandContext<'_>, ...)` takes it by shared reference and returns `Result<Message, IpcError>` — commands like `Type` (`require_active_overlay`), `Select`/`Edit` (`require_overlay`), `Undo` (`NothingToUndo` when the stack and any open session are both empty), and `Redo` (`NothingToRedo`, or `RedoWhileEditing` when a session is open with nothing left to redo) fail here, before any `Message` is built or `update()` runs. Because `to_message` only ever borrows `CommandContext` — it has no way to mutate `App` — there is no window between checking a precondition and dispatching the message where the state being checked could change out from under it (no TOCTOU): the borrow checker makes it structurally impossible to interleave a mutation between the check and the dispatch, rather than relying on discipline to keep them adjacent.

**Post-hoc outcomes are reported through `last_command_error`/`last_command_warning`.** Some failures are only knowable after a handler actually runs — opening a file that doesn't exist, or saving to a destination the writer rejects — because they depend on results of I/O the precondition check has no way to predict from state alone. Handlers that do this fallible work set `App::last_command_error` (or `last_command_warning` for a non-fatal outcome) synchronously during `update()`. `App::command_response` (`src/app/mod.rs`) folds whatever was set into the reply after the command's `update()` call returns, and `run_ipc_command` clears both fields at the start of every command so a stale error from a previous command can never be blamed on the next one.

Together, this means a `to_message` success plus a clean `command_response` is the actual contract an IPC caller can rely on: the command was dispatched *and* whatever fallible work it triggered didn't fail.

## Trade-offs

**Chosen: shared `CommandContext` + `to_message` preconditions, plus `last_command_error`/`warning` for post-hoc outcomes**
- One place (`CommandContext`) enumerates the state every precondition needs, instead of each command reaching into `App` directly
- The borrow-checker guarantee against TOCTOU is a property of the design, not a convention that has to be maintained by hand in every new command
- Two mechanisms for two different failure timings (before-dispatch vs. after-running) is slightly more surface area than one, but conflating them would either force every check to happen after a `Message` already ran (losing the "never dispatch something the state rules out" guarantee) or force every I/O failure to be predicted before it happens (impossible)

**Rejected: per-command bespoke fields**
- E.g. `SaveResult`, `TypeResult` as separate response fields per command, or ad hoc booleans threaded through individual handlers
- Would require touching the response type (and every caller of it) for each new command that can fail in its own way
- No shared place to see or test the full set of preconditions a command is subject to; `CommandContext` and `IpcError` currently serve as that single enumeration
