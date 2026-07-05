use crate::error::AppError;

use super::{fetch_bytes, fetch_json};

const TOKEN_ENV: &str = "BITBUCKET_TOKEN";

fn auth(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match std::env::var(TOKEN_ENV) {
        Ok(token) if !token.is_empty() => request.header("Authorization", format!("Bearer {token}")),
        _ => request,
    }
}

pub async fn default_branch(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> Result<String, AppError> {
    let url = format!("https://api.bitbucket.org/2.0/repositories/{owner}/{repo}");

    let body = fetch_json(auth(client.get(&url)), || {
        format!("repository {owner}/{repo} not found on bitbucket")
    })
    .await?;

    body.get("mainbranch")
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Upstream("missing mainbranch.name in bitbucket response".into()))
}

pub async fn download_tarball(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Vec<u8>, AppError> {
    let url = format!("https://bitbucket.org/{owner}/{repo}/get/{branch}.tar.gz");

    fetch_bytes(auth(client.get(&url)), || {
        format!("branch '{branch}' not found in {owner}/{repo} on bitbucket")
    })
    .await
}
