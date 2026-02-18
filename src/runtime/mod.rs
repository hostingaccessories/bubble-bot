pub mod go;
pub mod node;
pub mod php;
pub mod rust;

pub trait Runtime {
    fn name(&self) -> &str;
    fn template(&self) -> &str;
}
