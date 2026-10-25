#[path = "error.rs"]
pub mod error;
#[path = "@SRC@/config.rs"]
pub mod config;
#[path = "@SRC@/util.rs"]
pub mod util;
#[path = "@SRC@/engine/mod.rs"]
pub mod engine;
pub mod modrinth {
    pub fn client() -> &'static reqwest::Client {
        static C: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        C.get_or_init(|| reqwest::Client::builder().user_agent("flsp-test").build().unwrap())
    }
}
pub mod auth {
    #[derive(Clone)]
    pub struct Account { pub name: String, pub uuid: String, pub kind: String, pub xuid: String }
    pub async fn refresh_mc_token(_u: &str) -> Result<String, String> { Err("stub".into()) }
    pub fn get_mc_token(_u: &str) -> Result<String, String> { Err("stub".into()) }
}
pub mod launch;
