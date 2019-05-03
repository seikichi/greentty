mod action;
mod display;
mod handlers;
mod pty;
mod state;
mod store;
mod update;

use std::error::Error;
use std::sync::mpsc::channel;

use action::Action;
use handlers::{DisplayHandler, PtyHandler};
use state::State;
use store::Store;
use update::update;

fn main() -> Result<(), Box<Error>> {
    let cols = 80;
    let rows = 24;
    let (tx, rx) = channel::<Action>();

    let pty = pty::Pty::spawn(
        &pty::Config {
            shell: "powershell",
            cols,
            rows,
            ..Default::default()
        },
        PtyHandler { tx: tx.clone() },
    )?;
    let mut display = display::Display::open(DisplayHandler {
        pty: pty.clone(),
        tx: tx.clone(),
    })?;
    let mut store = Store::new(update, State::new());

    loop {
        let action = rx.recv()?;
        if let Action::Close() = action {
            break;
        }
        store.dispatch(&action);
        for action in rx.try_iter() {
            store.dispatch(&action);
        }
        display.render(store.get_state())?;
    }

    Ok(())
}
