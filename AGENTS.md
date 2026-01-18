# AGENTS.md

## 0. Golden Rules (CRITICAL)

These rules are non-negotiable. Violating them breaks the CI pipeline.

1. **Format with Nightly:** You MUST use `cargo +nightly fmt`. Never run `cargo fmt` without `+nightly`.
2. **Strict Linting:** Code must pass `cargo clippy --all-features -- -Dwarnings`. No warnings allowed.
3. **Sync Tests & Docs:** If you change behavior, you MUST update the corresponding test (colocated `#[test]`) AND any related documentation in `docs/src/`.
4. **No `unwrap()`:** In `src/`, strictly avoid `.unwrap()` or `.expect()`. Use `?` propagation with the `AppError` type.
5. **Compile-Time Verification:** Leverage the type system to make invalid states unrepresentable. Prefer parsing into strong types over validating loose types.
6. **Atomic Commits:** When asked to implement a feature, keep changes minimal and focused. Do not refactor unrelated code unless explicitly asked to.

## 1. Project Identity

- **Project:** `intelli-shell`
- **Description:** A smart bookmark manager and IntelliSense-like tool for shells (TUI + CLI).
- **Role:** Principal Rust Engineer. You prioritize performance, type safety, and clean architecture.
- **Stack:**
  - **Language:** Rust (2024 Edition).
  - **TUI:** `ratatui` (Rendering), `crossterm` (Events).
  - **CLI:** `clap` (Arguments).
  - **Async:** `tokio` (Runtime), `async-trait`.
  - **DB:** `rusqlite` bundled with `sea-query`.

## 2. Architecture & Patterns

### A. The CLI / Process Separation

The project separates **Data Definition** from **Execution Logic**.

1. **Definitions (`src/cli.rs`):**
   - All CLI commands are structs defined here.
   - They derive `clap::Parser` or `clap::Args`.
   - *Rule:* **Pure Data Only.** Do not put business logic or `impl` blocks here.

2. **Implementations (`src/process/*.rs`):**
   - The execution logic lives here.
   - You implement the `Process` (headless) or `InteractiveProcess` (TUI) trait **for** the structs defined in `cli.rs`.
   - *Example:* `src/process/search.rs` contains `impl InteractiveProcess for crate::cli::SearchCommand`.

### B. The Component Pattern (`src/component/`)

The TUI is built on the `Component` trait (`src/component/mod.rs`).

1. **Async Trait:** Implementations must be `#[async_trait]`.
2. **Input Routing:**
   - **NEVER** hardcode key checks directly in `process_key_event` without a fallback.
   - **ALWAYS** call `self.default_process_key_event(keybindings, key).await?` first.
   - *Reason:* This respects user-configured keybindings and standard navigation (Vim/Emacs).
3. **State Flow:** Components return `Result<Action>`.
   - `Action::SwitchComponent(...)`: Transitions to a new screen.
   - `Action::Quit(ProcessOutput::...)`: Exits the application.

### C. Reusable Widgets (`src/widgets/`)

Reusable UI parts (pure rendering logic) reside in `src/widgets/`.

- **Definition:** These are `ratatui` Widgets or helper structs that define *how* something looks.
- **Usage:** Components (`src/component/`) should consume these widgets to render their state.
- **Goal:** Ensure visual consistency and allow multiple components to share UI elements (e.g., lists, text inputs, help footers) without duplicating code.

### D. Error Handling Strategy

- **File:** `src/errors.rs`
- **Type:** `AppError` (Do **not** use `thiserror` or `anyhow`).
- **Variants:**
  - `UserFacing(UserFacingError)`: For expected logic errors (e.g., "Item not found"). Displayed cleanly.
  - `Unexpected(color_eyre::Report)`: For system failures. Generates stack traces.

## 3. Workflows & Commands

Match the CI environment exactly.

| Action     | Command                                     | Notes                                |
| :--------- | :------------------------------------------ | :----------------------------------- |
| **Format** | `cargo +nightly fmt`                        | **CRITICAL.** Uses nightly features. |
| **Lint**   | `cargo clippy --all-features -- -Dwarnings` | Enforces clean code (no warnings).   |
| **Test**   | `cargo test --all-features`                 | Runs all unit and integration tests. |
| **Build**  | `cargo build`                               | Standard dev build.                  |
| **Run**    | `cargo run -- <args>`                       | Example: `cargo run -- changelog`    |

## 4. Project Structure

The `src/lib.rs` file defines the authoritative module tree.

### Core Infrastructure

- **`src/app.rs`**: Main coordinator. Orchestrates the TUI loop.
- **`src/cli.rs`**: **Source of Truth** for CLI arguments/flags.
- **`src/config.rs`**: Configuration loading and settings management.
- **`src/logging.rs`**: Tracing setup and log file management.
- **`src/errors.rs`**: Centralized error handling (`AppError`) and panic hooks.
- **`src/tui.rs`**: Low-level terminal setup, event loop, and rendering backend.

### Domain & Logic

- **`src/model/`**: Core domain entities (Commands, Variables, etc.).
- **`src/process/`**: Bridge between CLI args and Service logic.
- **`src/service/`**: Pure business logic (Platform agnostic). **Start here** for logic changes.
- **`src/storage/`**: Data access layer (SQLite implementation).
- **`src/ai/`**: LLM client integration and logic.

### UI & Utilities

- **`src/component/`**: Interactive TUI components (Logic + Input).
- **`src/widgets/`**: Reusable TUI widgets (Rendering only).
- **`src/utils/`**: General purpose helpers (Strings, Markdown, etc.).
- **`docs/`**: mdBook documentation source.

## 5. Behavior & Thinking Process

1. **Analyze first:** Identify the relevant `cli` struct (data) and `process` implementation (logic). Trace the flow from `process` -> `service` -> `storage` before writing code.
2. **Check Dependencies:** Check `Cargo.toml`. Prefer existing dependencies (`tokio`, `itertools`, `uuid`) over new ones.
3. **Verify Tests:** After writing code, ask yourself: "Did I break tests in `src/process/` or `src/service/`?"
4. **Verify Documentation:** After writing code, ask yourself: "Did I break docs in `docs/src/`?"
