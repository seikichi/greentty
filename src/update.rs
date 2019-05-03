use crate::action::Action;
use crate::state::State;

pub fn update(state: &mut State, action: &Action) {
    match action {
        Action::Print(c) => {
            while state.cursor.y >= state.lines.len() {
                state.lines.push(vec![]);
            }
            while state.cursor.x >= state.lines[state.cursor.y].len() {
                state.lines[state.cursor.y].push(' ');
            }
            state.lines[state.cursor.y][state.cursor.x] = *c;
            state.cursor.x += 1;
        }
        Action::Execute(byte) => {
            match byte {
                8 /* BS */ => {
                    state.cursor.x -= 1;
                }
                10 /* LF */ => {
                    state.cursor.y += 1;
                }
                13 /* CR */ => {
                    state.cursor.x = 0;
                }
                _ => {}
            }
        }
        Action::DispatchCSI(params, _intermediates, _ignore, c) => {
            match c {
                'H' => {
                    let y = if params.len() >= 1 { params[0] - 1 } else { 0 } as usize;
                    let x = if params.len() >= 2 { params[1] - 1 } else { 0 } as usize;
                    state.cursor.y = y;
                    state.cursor.x = x;
                }
                'X' => {
                    // FIXME
                    // let s = if params.len() >= 1 { params[0] } else { 1 } as usize;
                    while state.cursor.y >= state.lines.len() {
                        state.lines.push(vec![]);
                    }
                    state.lines[state.cursor.y].resize(state.cursor.x, ' ');
                }
                'C' => {
                    let s = if params.len() >= 1 { params[0] } else { 1 } as usize;
                    state.cursor.x += s;
                }
                _ => {}
            }
        }
        _ => {}
    }
}
