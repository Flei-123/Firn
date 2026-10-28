use crate::target;
use std::cell::Cell;

thread_local! {
    static MODE: Cell<Option<bool>> = const { Cell::new(None) };
}

pub fn set(v: bool) {
    MODE.with(|c| c.set(Some(v)));
}

fn default_on() -> bool {
    target::active() == target::Target::X86_64
        && !target::windows()
        && std::env::var_os("FIRN_USE_AS").is_none()
}

pub fn get() -> bool {
    MODE.with(|c| c.get()).unwrap_or_else(default_on)
}
