#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Print(char),
    Execute(u8),
    DispatchCSI(Vec<i64>, Vec<u8>, bool, char),
    Close(),
}
