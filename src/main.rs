//! Chronos Executable Entry Point (`main.rs`).
//!
//! Sets up panic-safe terminal raw-mode management, dispatches CLI subcommands when provided,
//! or launches the 20Hz interactive Material Design 3 TUI.

use chronos::cli::{execute_cli_command, CliArgs};
use chronos::error::ChronosResult;
use chronos::keymap::handle_key_event;
use chronos::runtime::WorkspaceRuntime;
use chronos::tui::shell::render_workspace_frame;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::panic;
use std::time::{Duration, Instant};

const TICK_INTERVAL: Duration = Duration::from_millis(50);

fn restore_terminal_state() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn main() -> ChronosResult<()> {
    let args = CliArgs::parse();
    if let Some(cmd) = args.command {
        return execute_cli_command(cmd);
    }

    run_interactive_tui()
}

fn run_interactive_tui() -> ChronosResult<()> {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal_state();
        default_hook(info);
    }));

    let mut runtime = WorkspaceRuntime::bootstrap()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(err.into());
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(err) => {
            restore_terminal_state();
            return Err(err.into());
        }
    };

    let loop_result = event_loop(&mut terminal, &mut runtime);

    runtime.clean_shutdown();
    restore_terminal_state();
    loop_result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    runtime: &mut WorkspaceRuntime,
) -> ChronosResult<()> {
    let mut last_tick = Instant::now();

    while !runtime.should_quit {
        terminal.draw(|frame| render_workspace_frame(frame, runtime))?;

        let timeout = TICK_INTERVAL.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // Ignore key release events on terminals that report them
                if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                    handle_key_event(runtime, key);
                }
            }
        }

        if last_tick.elapsed() >= TICK_INTERVAL {
            runtime.on_tick();
            last_tick = Instant::now();
        }
    }

    Ok(())
}
