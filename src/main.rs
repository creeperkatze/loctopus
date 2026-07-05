mod error;
mod locs;
mod platform;

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use moka::future::Cache;
use regex::Regex;
use serde::Deserialize;
use serde_json::json;
use tower_http::trace::TraceLayer;

use error::AppError;
use platform::{ForgeClient, Platform};

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60); // 1 day

#[derive(Clone)]
struct AppState {
    platform_client: Arc<ForgeClient>,
    cache: Cache<CacheKey, Arc<locs::Locs>>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    platform: Platform,
    owner: String,
    repo: String,
    branch: String,
    filter: Option<String>,
}

#[derive(Deserialize)]
struct LocsQuery {
    branch: Option<String>,
    filter: Option<String>,
}

#[derive(Deserialize)]
struct BadgeQuery {
    branch: Option<String>,
    filter: Option<String>,
    format: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let state = AppState {
        platform_client: Arc::new(ForgeClient::new()),
        cache: Cache::builder().time_to_live(CACHE_TTL).build(),
    };

    let app = Router::new()
        .route("/:platform/:owner/:repo", get(get_locs))
        .route("/:platform/:owner/:repo/badge", get(get_badge))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind port");

    tracing::info!("listening on http://0.0.0.0:{port}");

    axum::serve(listener, app).await.expect("server error");
}

fn parse_platform(platform: &str) -> Result<Platform, AppError> {
    Platform::parse(platform).ok_or_else(|| {
        AppError::NotFound(format!(
            "unsupported platform '{platform}' (expected 'github', 'codeberg', 'gitlab', or 'bitbucket')"
        ))
    })
}

async fn resolve_branch(
    state: &AppState,
    platform: Platform,
    owner: &str,
    repo: &str,
    branch: Option<String>,
) -> Result<String, AppError> {
    match branch {
        Some(branch) => Ok(branch),
        None => {
            state
                .platform_client
                .default_branch(platform, owner, repo)
                .await
        }
    }
}

// Resolves platform/owner/repo/branch/filter to a `Locs` tree, serving from the in-memory TTL cache when possible and populating it on a miss.
async fn get_locs_cached(
    state: &AppState,
    platform: Platform,
    owner: String,
    repo: String,
    branch: Option<String>,
    filter: Option<String>,
) -> Result<Arc<locs::Locs>, AppError> {
    let branch = resolve_branch(state, platform, &owner, &repo, branch).await?;
    let filters = locs::parse_filters(filter.as_deref())?;

    let key = CacheKey {
        platform,
        owner: owner.clone(),
        repo: repo.clone(),
        branch: branch.clone(),
        filter,
    };

    if let Some(cached) = state.cache.get(&key).await {
        tracing::debug!(platform = platform.as_str(), %owner, %repo, %branch, "cache hit");
        return Ok(cached);
    }

    // Run on an independent task so a client disconnecting mid-request doesn't cancel the work: the repo still gets downloaded, processed, and cached either way.
    let state = state.clone();
    let handle = tokio::spawn(async move {
        let tarball = state
            .platform_client
            .download_tarball(platform, &owner, &repo, &branch)
            .await?;

        let start = std::time::Instant::now();
        let result = compute_locs_blocking(tarball, filters).await?;

        tracing::info!(platform = platform.as_str(), %owner, %repo, %branch, loc = result.loc, duration_ms = start.elapsed().as_millis(), "computed locs");

        let result = Arc::new(result);
        state.cache.insert(key, Arc::clone(&result)).await;

        Ok::<Arc<locs::Locs>, AppError>(result)
    });

    handle
        .await
        .map_err(|e| AppError::Upstream(format!("locs task panicked: {e}")))?
}

// Decompressing and walking a large tarball is CPU/IO-heavy synchronous work; run it on the blocking thread pool so it doesn't stall the async runtime's worker threads for other in-flight requests.
async fn compute_locs_blocking(
    tarball: Vec<u8>,
    filters: Vec<Regex>,
) -> Result<locs::Locs, AppError> {
    tokio::task::spawn_blocking(move || locs::compute_locs(&tarball, &filters))
        .await
        .map_err(|e| AppError::Upstream(format!("locs computation panicked: {e}")))?
}

async fn get_locs(
    State(state): State<AppState>,
    Path((platform, owner, repo)): Path<(String, String, String)>,
    Query(query): Query<LocsQuery>,
) -> Result<Json<Arc<locs::Locs>>, AppError> {
    let platform = parse_platform(&platform)?;
    let result = get_locs_cached(&state, platform, owner, repo, query.branch, query.filter).await?;
    Ok(Json(result))
}

async fn get_badge(
    State(state): State<AppState>,
    Path((platform, owner, repo)): Path<(String, String, String)>,
    Query(query): Query<BadgeQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let platform = parse_platform(&platform)?;
    let result = get_locs_cached(&state, platform, owner, repo, query.branch, query.filter).await?;

    let message = if query.format.as_deref() == Some("human") {
        locs::humanize(result.loc)
    } else {
        result.loc.to_string()
    };

    Ok(Json(json!({
        "schemaVersion": 1,
        "label": "lines",
        "message": message,
        "cacheSeconds": CACHE_TTL.as_secs(),
    })))
}
