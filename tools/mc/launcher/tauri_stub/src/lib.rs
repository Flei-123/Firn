#[derive(Clone)]
pub struct Window;
pub trait Emitter {
    fn emit<S: serde::Serialize>(&self, _event: &str, _payload: S) -> Result<(), String>;
}
impl Emitter for Window {
    fn emit<S: serde::Serialize>(&self, _event: &str, _payload: S) -> Result<(), String> {
        Ok(())
    }
}
