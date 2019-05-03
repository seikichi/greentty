#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub cursor: Position,
    pub lines: Vec<Vec<char>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

impl State {
    pub fn new() -> Self {
        Self {
            cursor: Position { x: 0, y: 0 },
            lines: vec![],
        }
    }
}
