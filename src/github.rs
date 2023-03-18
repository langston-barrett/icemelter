use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::env::{var, VarError};

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent("langston-barrett/icemelter")
        .build()
        .unwrap()
});

pub(crate) struct Config {
    token: String,
}

impl Config {
    pub(crate) const ENV_VAR: &str = "GITHUB_TOKEN";

    pub(crate) fn from_env() -> Result<Self, VarError> {
        Ok(Self {
            token: var(Self::ENV_VAR)?,
        })
    }
}

pub(crate) fn get_issue(config: &Config, number: usize) -> Result<Issue, reqwest::Error> {
    let url = format!("https://api.github.com/repos/rust-lang/rust/issues/{number}");
    CLIENT
        .get(url)
        .bearer_auth(&config.token)
        .send()?
        .error_for_status()?
        .json()
}

#[derive(Deserialize, Debug)]
pub(crate) struct Issue {
    pub(crate) number: usize,
    pub(crate) body: String,
}
