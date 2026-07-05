use crate::error::AppError;

use super::{fetch_bytes, fetch_json};

const TOKEN_ENV: &str = "GITLAB_TOKEN";

fn auth(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match std::env::var(TOKEN_ENV) {
        Ok(token) if !token.is_empty() => request.header("PRIVATE-TOKEN", token),
        _ => request,
    }
}

// GitLab's API addresses projects by their URL-encoded `owner/repo` path.
fn project_path(owner: &str, repo: &str) -> String {
    format!("{owner}%2F{repo}")
}

pub async fn default_branch(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> Result<String, AppError> {
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}",
        project_path(owner, repo)
    );

    let body = fetch_json(auth(client.get(&url)), || {
        format!("repository {owner}/{repo} not found on gitlab")
    })
    .await?;

    body.get("default_branch")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::Upstream("missing default_branch in gitlab response".into()))
}

pub async fn download_tarball(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Vec<u8>, AppError> {
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}/repository/archive.tar.gz?sha={branch}",
        project_path(owner, repo)
    );

    fetch_bytes(auth(client.get(&url)), || {
        format!("branch '{branch}' not found in {owner}/{repo} on gitlab")
    })
    .await
}
