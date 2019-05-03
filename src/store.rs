pub struct Store<S, A> {
    state: S,
    updater: fn(&mut S, &A) -> (),
}

impl<S, A> Store<S, A> {
    pub fn new(updater: fn(&mut S, &A) -> (), state: S) -> Self {
        Store { state, updater }
    }

    pub fn dispatch(&mut self, action: &A) {
        (self.updater)(&mut self.state, action);
    }

    pub fn get_state(&self) -> &S {
        &self.state
    }
}
