//! The app's tiny **network layer** — real HTTP + JSON.
//!
//! The feed, its comments, and the people who wrote them all come from
//! [dummyjson.com](https://dummyjson.com), a free public test API. Everything here is
//! **blocking**: each `fetch_*` runs a synchronous HTTP GET and decodes the JSON into
//! a typed struct. That's deliberate — the store calls these functions from inside
//! `spawn(...)`, which runs them on a background thread and hands the result back to
//! the UI thread on a later frame. So the UI never blocks; this file just describes
//! *what* to fetch, and the store owns *when*.
//!
//! Swap `BASE` for your own API and the rest of the app is unchanged.

use std::time::Duration;

use serde::Deserialize;

const BASE: &str = "https://dummyjson.com";

/// A short-timeout HTTP agent (same client `ImageView` uses for network images).
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()
}

/// GET `url` and decode its JSON body into `T`. Blocking — only ever called from a
/// background thread via `spawn`. Any failure (network, HTTP status, bad JSON) comes
/// back as a human-readable `Err(String)` the UI can show.
fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let body =
        agent().get(url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Posts — one page of the feed
// ---------------------------------------------------------------------------

/// One page of feed posts plus the grand `total` (so we know when to stop paging).
#[derive(Deserialize)]
pub struct PostPage {
    pub posts: Vec<ApiPost>,
    #[serde(default)]
    pub total: u32,
}

/// A post as the API returns it. We only decode the fields the UI uses.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiPost {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub reactions: Reactions,
    pub user_id: u64,
}

/// A post's reaction counts (we surface `likes`).
#[derive(Deserialize, Default)]
pub struct Reactions {
    #[serde(default)]
    pub likes: u32,
}

/// Fetch one page of the feed: `limit` posts starting at offset `skip`.
pub fn fetch_posts(skip: u32, limit: u32) -> Result<PostPage, String> {
    get_json(&format!("{BASE}/posts?limit={limit}&skip={skip}&select=title,body,reactions,userId"))
}

// ---------------------------------------------------------------------------
// Comments — one post's thread
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CommentPage {
    comments: Vec<ApiComment>,
}

/// A comment as the API returns it (with its author inlined).
#[derive(Deserialize)]
pub struct ApiComment {
    pub id: u64,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub likes: u32,
    pub user: ApiCommentUser,
}

/// The commenter, as inlined on each comment.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiCommentUser {
    pub id: u64,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub username: String,
}

/// Fetch every comment on a post.
pub fn fetch_comments(post_id: u64) -> Result<Vec<ApiComment>, String> {
    get_json::<CommentPage>(&format!("{BASE}/posts/{post_id}/comments")).map(|p| p.comments)
}

// ---------------------------------------------------------------------------
// Users — the people who authored posts + comments
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct UserPage {
    users: Vec<ApiUser>,
}

/// A user profile (name + avatar) keyed by the same `id` posts/comments reference.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiUser {
    pub id: u64,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub username: String,
    /// A hosted avatar URL.
    #[serde(default)]
    pub image: String,
}

/// Fetch the whole user directory once (`limit=0` returns all), so any post or
/// comment author resolves to a real name + avatar.
pub fn fetch_users() -> Result<Vec<ApiUser>, String> {
    get_json::<UserPage>(&format!("{BASE}/users?limit=0&select=firstName,lastName,username,image"))
        .map(|p| p.users)
}
