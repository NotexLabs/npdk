use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub package: Package,
    pub profile: Profile,
}

#[derive(Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub exposes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Profile {
    pub build: String,
}
