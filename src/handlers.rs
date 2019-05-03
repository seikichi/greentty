use crate::action::Action;
use crate::display;
use crate::pty;

use std::sync::mpsc::Sender;

pub struct PtyHandler {
    pub tx: Sender<Action>,
}

impl pty::Handler for PtyHandler {
    fn print(&mut self, c: char) {
        self.tx.send(Action::Print(c)).unwrap();
    }
    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let action = Action::DispatchCSI(params.to_vec(), intermediates.to_vec(), ignore, c);
        self.tx.send(action).unwrap()
    }
    fn execute(&mut self, byte: u8) {
        self.tx.send(Action::Execute(byte)).unwrap();
    }
    fn hook(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]]) {}
    fn esc_dispatch(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

pub struct DisplayHandler {
    pub pty: pty::Pty,
    pub tx: Sender<Action>,
}

impl display::Handler for DisplayHandler {
    fn on_window_event(&mut self, event: &display::WindowEvent) {
        use glium::glutin::*;
        match event {
            WindowEvent::CloseRequested => self.tx.send(Action::Close()).unwrap(),
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        state: ElementState::Pressed,
                        virtual_keycode: Some(keypress),
                        ..
                    },
                ..
            } => match keypress {
                VirtualKeyCode::Escape => self.tx.send(Action::Close()).unwrap(),
                _ => (),
            },
            WindowEvent::ReceivedCharacter(c) => {
                let mut bytes = [0; 4];
                let s = c.encode_utf8(&mut bytes);
                for b in s.bytes() {
                    self.pty.write(b).unwrap();
                }
            }
            _ => {}
        }
    }
}
