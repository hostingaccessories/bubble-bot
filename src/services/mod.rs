pub mod mysql;
pub mod postgres;
pub mod redis;

pub trait Service {
    fn name(&self) -> &str;
    fn image(&self) -> String;
    fn env_vars(&self) -> Vec<(String, String)>;
}
