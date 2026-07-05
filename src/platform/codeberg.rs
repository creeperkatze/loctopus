use crate::error::AppError;

use super::{fetch_bytes, fetch_json};

const TOKEN_ENV: &str = "CODEBERG_TOKEN";

fn auth(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match std::env::var(TOKEN_ENV) {
        Ok(token) if !token.is_empty() => request.header("Authorization", format!("token {token}")),
        _ => request,
    }
}

pub async fn default_branch(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> Result<String, AppError> {
    let url = format!("https://codeberg.org/api/v1/repos/{owner}/{repo}");

    let body = fetch_json(auth(client.get(&url)), || {
        format!("repository {owner}/{repo} not found on codeberg")
    })
    .await?;

    body.get("default_branch")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Upstream("missing default_branch in codeberg response".into()))
}

pub async fn download_tarball(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Vec<u8>, AppError> {
    let url = format!("https://codeberg.org/{owner}/{repo}/archive/{branch}.tar.gz");

    fetch_bytes(auth(client.get(&url)), || {
        format!("branch '{branch}' not found in {owner}/{repo} on codeberg")
    })
    .await
}
