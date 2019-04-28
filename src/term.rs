use vte::{Parser, Perform};

pub struct Terminal {
    parser: Parser,
    state: State,
}

pub struct TerminalConfig {
    pub cols: u32,
    pub rows: u32,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        TerminalConfig { cols: 80, rows: 24 }
    }
}

impl Terminal {
    pub fn new(_config: &TerminalConfig) -> Self {
        Terminal {
            parser: Parser::new(),
            state: State {
                cursor: Position { x: 0, y: 0 },
                lines: vec![],
            },
        }
    }

    pub fn send(&mut self, byte: u8) {
        self.parser.advance(&mut self.state, byte)
    }

    pub fn lines(&self) -> Vec<String> {
        self.state
            .lines
            .iter()
            .map(|line| line.iter().collect::<String>())
            .collect::<Vec<String>>()
    }
}

struct Position {
    x: usize,
    y: usize,
}

struct State {
    cursor: Position,
    lines: Vec<Vec<char>>,
}

impl Perform for State {
    fn print(&mut self, c: char) {
        while self.cursor.y >= self.lines.len() {
            self.lines.push(vec![]);
        }
        while self.cursor.x >= self.lines[self.cursor.y].len() {
            self.lines[self.cursor.y].push(' ');
        }
        self.lines[self.cursor.y][self.cursor.x] = c;
        self.cursor.x += 1;
    }
    fn csi_dispatch(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool, c: char) {
        match c {
            'H' => {
                let y = if params.len() >= 1 { params[0] - 1 } else { 0 } as usize;
                let x = if params.len() >= 2 { params[1] - 1 } else { 0 } as usize;
                self.cursor.y = y;
                self.cursor.x = x;
            }
            'X' => {
                // FIXME
                // let s = if params.len() >= 1 { params[0] } else { 1 } as usize;
                while self.cursor.y >= self.lines.len() {
                    self.lines.push(vec![]);
                }
                self.lines[self.cursor.y].resize(self.cursor.x, ' ');
            }
            'C' => {
                let s = if params.len() >= 1 { params[0] } else { 1 } as usize;
                self.cursor.x += s;
            }
            _ => {}
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            8 /* BS */ => {
                self.cursor.x -= 1;
            }
            10 /* LF */ => {
                self.cursor.y += 1;
            }
            13 /* CR */ => {
                self.cursor.x = 0;
            }
            _ => {}
        }
    }
    fn hook(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]]) {}
    fn esc_dispatch(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}
