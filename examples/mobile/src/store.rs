//! The **app state manager** — every screen reads these signals and calls these
//! actions; nothing else mutates state.
//!
//! Two worlds live here, on purpose:
//!   * **Curated, in-memory** — your own profile, the notifications, the DM threads.
//!     Seeded below, edited locally. Instant, always present.
//!   * **Live from the network** — the feed, its comments, and the people who wrote
//!     them, fetched from a public test API (see [`net`]). These load *asynchronously*:
//!     an action fires a background fetch via `spawn`, and its result lands on the UI
//!     thread on a later frame by writing a signal — so the view re-renders itself.
//!
//! The feed pages in as you scroll (infinite scroll), comments load when you open a
//! post, and everything you do on top (like, reply, delete) is local state layered
//! over the fetched data. That's the whole illusion of a working social app.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use pebbles::prelude::*;

use crate::net;

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub handle: String,
    pub avatar: String, // network URL
    pub bio: String,
    pub followers: u32,
    pub following: u32,
    pub i_follow: bool, // does the current user follow them?
}

/// A comment (or, nested one level, a reply). Replies you add live in `replies`.
#[derive(Clone)]
pub struct Comment {
    pub id: u64,
    pub author: u64,
    pub text: String,
    pub likes: u32,
    pub liked: bool,
    pub replies: Vec<Comment>,
}

#[derive(Clone)]
pub struct Post {
    pub id: u64,
    pub author: u64,
    pub text: String,
    pub media: Option<String>, // network URL
    pub likes: u32,
    pub liked: bool,
    pub bookmarked: bool,
    /// How many comments the post has, once known (`None` until its thread loads).
    pub comment_count: Option<u32>,
    pub time: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotifKind {
    Like,
    Comment,
    Follow,
}

#[derive(Clone)]
pub struct Notif {
    pub kind: NotifKind,
    pub actor: u64,
    pub read: bool,
    pub time: String,
}

/// The lifecycle of an async load — shared by the feed and each comment thread.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadState {
    /// Nothing requested yet.
    #[default]
    Idle,
    /// A fetch is in flight (show a spinner).
    Loading,
    /// Resolved; more pages may still be available.
    Loaded,
    /// Fully loaded — no more pages.
    Done,
    /// The last fetch failed (offer a retry).
    Error,
}

/// The signed-in user's id (fixed — this is a demo).
pub const ME: u64 = 1;

/// Feed + comment authors coming from the API are stored under their id **plus this
/// offset**, so they can never collide with the curated seed users (ids 1–5).
const API_UID: u64 = 1_000_000;

/// Posts you create locally get ids at/above this, so they never collide with the
/// API's (small) post ids — and so we can tell a local post from a fetched one.
const LOCAL_POST_BASE: u64 = 900_000;

/// How many posts to pull per feed page.
const PAGE: u32 = 8;

// ---------------------------------------------------------------------------
// Free image helpers (no API key; load over the native HTTP client)
// ---------------------------------------------------------------------------

/// A free avatar from pravatar (deterministic by `n`, 1..=70).
fn avatar(n: u32) -> String {
    format!("https://i.pravatar.cc/150?img={n}")
}

/// A free photo from Lorem Picsum (deterministic by `seed`).
pub fn photo(seed: &str) -> String {
    format!("https://picsum.photos/seed/{seed}/640/440")
}

// ---------------------------------------------------------------------------
// State — app-scoped signals (survive tab switches) + a few plain counters
// ---------------------------------------------------------------------------

/// One comment thread: its load state + the comments themselves.
#[derive(Clone, Default)]
struct Thread {
    state: LoadState,
    items: Vec<Comment>,
}

thread_local! {
    static SEED_USERS: RefCell<Option<Signal<Vec<User>>>> = const { RefCell::new(None) };
    static API_USERS: RefCell<Option<Signal<HashMap<u64, User>>>> = const { RefCell::new(None) };
    static FEED: RefCell<Option<Signal<Vec<Post>>>> = const { RefCell::new(None) };
    static FEED_STATE: RefCell<Option<Signal<LoadState>>> = const { RefCell::new(None) };
    static COMMENTS: RefCell<Option<Signal<HashMap<u64, Thread>>>> = const { RefCell::new(None) };
    static NOTIFS: RefCell<Option<Signal<Vec<Notif>>>> = const { RefCell::new(None) };
    static POST_ROUTE: RefCell<Option<Signal<Option<u64>>>> = const { RefCell::new(None) };

    static NEXT_POST_ID: Cell<u64> = const { Cell::new(LOCAL_POST_BASE + 2) };
    static NEXT_COMMENT_ID: Cell<u64> = const { Cell::new(10_000_000) };
    static FEED_SKIP: Cell<u32> = const { Cell::new(0) };
    static FEED_TOTAL: Cell<u32> = const { Cell::new(0) };
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

fn seed_users_sig() -> Signal<Vec<User>> {
    SEED_USERS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_users())))
}
fn api_users() -> Signal<HashMap<u64, User>> {
    API_USERS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(HashMap::new())))
}
fn feed_sig() -> Signal<Vec<Post>> {
    FEED.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_feed())))
}
fn feed_state_sig() -> Signal<LoadState> {
    FEED_STATE.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(LoadState::Idle)))
}
fn comments_sig() -> Signal<HashMap<u64, Thread>> {
    COMMENTS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_comments())))
}
pub fn notifs() -> Signal<Vec<Notif>> {
    NOTIFS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_notifs())))
}
fn post_route() -> Signal<Option<u64>> {
    POST_ROUTE.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(None)))
}

// ---------------------------------------------------------------------------
// Reads / lookups
// ---------------------------------------------------------------------------

/// Look up a user by id — curated seed users (1–5) or a fetched API author.
pub fn user(id: u64) -> User {
    if id >= API_UID {
        api_users().get().get(&id).cloned().unwrap_or_else(|| loading_user(id))
    } else {
        seed_users_sig().get().into_iter().find(|u| u.id == id).unwrap_or_else(|| loading_user(id))
    }
}

pub fn me() -> User {
    user(ME)
}

/// The feed — the local seed posts followed by everything paged in from the API.
pub fn feed() -> Vec<Post> {
    feed_sig().get()
}

/// Where the feed's paging is up to (drives the footer spinner / "all caught up").
pub fn feed_state() -> LoadState {
    feed_state_sig().get()
}

/// The current user's own posts (for the profile grid).
pub fn my_posts() -> Vec<Post> {
    feed_sig().get().into_iter().filter(|p| p.author == ME).collect()
}

/// A single post by id.
pub fn post(id: u64) -> Option<Post> {
    feed_sig().get().into_iter().find(|p| p.id == id)
}

/// A post's comment thread: its load state + the comments (reactive).
pub fn comment_thread(post_id: u64) -> (LoadState, Vec<Comment>) {
    comments_sig()
        .get()
        .get(&post_id)
        .map(|t| (t.state, t.items.clone()))
        .unwrap_or((LoadState::Loading, Vec::new()))
}

/// Unread notification count (drives the tab badge).
pub fn unread() -> usize {
    notifs().get().iter().filter(|n| !n.read).count()
}

/// Which post (if any) is open full-screen in the detail view.
pub fn post_open() -> Option<u64> {
    post_route().get()
}

// ---------------------------------------------------------------------------
// Async loading — the feed pages in; comments load on demand
// ---------------------------------------------------------------------------

/// Kick off the first feed page + the user directory, exactly once. Called from the
/// feed screen's mount effect; safe to call on every re-mount (it self-guards).
pub fn ensure_feed_started() {
    if STARTED.with(Cell::get) {
        return;
    }
    STARTED.with(|c| c.set(true));
    load_users();
    load_feed_more();
}

/// Fetch the next feed page in the background, appending the results. No-op while a
/// fetch is already in flight or once every page has loaded — so the infinite-scroll
/// trigger can fire freely without stacking duplicate requests.
pub fn load_feed_more() {
    let state = feed_state_sig();
    if matches!(state.peek(), LoadState::Loading | LoadState::Done) {
        return;
    }
    state.set(LoadState::Loading);
    let skip = FEED_SKIP.with(Cell::get);

    // `net::fetch_posts` runs on a background thread; the closure runs back on the UI
    // thread once it resolves, where it may write signals.
    spawn(
        move || net::fetch_posts(skip, PAGE),
        move |result| match result {
            Ok(page) => {
                FEED_TOTAL.with(|c| c.set(page.total));
                let posts: Vec<Post> = page.posts.iter().map(map_post).collect();
                let got = posts.len();
                feed_sig().update(|v| v.extend(posts));

                let next = skip + PAGE;
                FEED_SKIP.with(|c| c.set(next));
                let done = got == 0 || (page.total > 0 && next >= page.total);
                feed_state_sig().set(if done { LoadState::Done } else { LoadState::Loaded });
            }
            Err(_) => feed_state_sig().set(LoadState::Error),
        },
    );
}

/// Fetch the whole user directory once, so post/comment authors resolve to real
/// names + avatars. Fire-and-forget: authors show a placeholder until it lands.
fn load_users() {
    spawn(net::fetch_users, |result| {
        if let Ok(list) = result {
            api_users().update(|map| {
                for u in list {
                    map.insert(API_UID + u.id, map_user(u));
                }
            });
        }
    });
}

/// Load a post's comments the first time its detail screen opens. Local posts already
/// hold their comments in memory; API posts fetch theirs. Self-guards against
/// re-loading, so the detail screen's mount effect can call it unconditionally.
pub fn load_comments(post_id: u64) {
    let existing = comments_sig().peek().get(&post_id).map(|t| t.state);
    if matches!(existing, Some(LoadState::Loading | LoadState::Loaded | LoadState::Done)) {
        return;
    }
    // A post you wrote locally: its comments are already here — just mark it ready.
    if post_id >= LOCAL_POST_BASE {
        comments_sig().update(|m| m.entry(post_id).or_default().state = LoadState::Loaded);
        return;
    }
    // An API post: fetch its thread off the UI thread.
    comments_sig().update(|m| m.entry(post_id).or_default().state = LoadState::Loading);
    spawn(
        move || net::fetch_comments(post_id),
        move |result| match result {
            Ok(list) => {
                // Seed the directory from each comment's inlined author, so names show
                // even if the full user fetch is slow (or failed). The directory's own
                // fetch overwrites these later with real avatars.
                register_comment_authors(&list);
                let items: Vec<Comment> = list.iter().map(map_comment).collect();
                let count = items.len() as u32;
                comments_sig().update(|m| {
                    let t = m.entry(post_id).or_default();
                    t.items = items;
                    t.state = LoadState::Loaded;
                });
                set_comment_count(post_id, count);
            }
            Err(_) => comments_sig().update(|m| m.entry(post_id).or_default().state = LoadState::Error),
        },
    );
}

// ---------------------------------------------------------------------------
// Actions — the only way anything changes
// ---------------------------------------------------------------------------

/// Publish a new post (optionally with an attached photo). Prepends to the feed.
pub fn create_post(text: &str, media: Option<String>) {
    let text = text.trim();
    if text.is_empty() && media.is_none() {
        return;
    }
    let id = NEXT_POST_ID.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    let post = Post {
        id,
        author: ME,
        text: text.to_string(),
        media,
        likes: 0,
        liked: false,
        bookmarked: false,
        comment_count: Some(0),
        time: "now".into(),
    };
    feed_sig().update(|v| v.insert(0, post));
    comments_sig().update(|m| m.entry(id).or_default().state = LoadState::Loaded);
    toast("Posted").show();
}

/// Like / unlike a post (updates the count + the heart).
pub fn toggle_like(id: u64) {
    edit_post(id, |p| {
        p.liked = !p.liked;
        if p.liked {
            p.likes += 1;
        } else {
            p.likes = p.likes.saturating_sub(1);
        }
    });
}

/// Bookmark / un-bookmark a post.
pub fn toggle_bookmark(id: u64) {
    let now_saved = edit_post(id, |p| p.bookmarked = !p.bookmarked);
    toast(if now_saved { "Saved" } else { "Removed" }).duration(1.2).show();
}

/// Add a top-level comment to a post (newest first).
pub fn add_comment(post_id: u64, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let comment = new_local_comment(text);
    comments_sig().update(|m| {
        let t = m.entry(post_id).or_default();
        t.items.insert(0, comment);
        if t.state == LoadState::Idle {
            t.state = LoadState::Loaded;
        }
    });
    bump_comment_count(post_id, 1);
}

/// Reply to a comment (nested one level under it).
pub fn add_reply(post_id: u64, parent_id: u64, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let reply = new_local_comment(text);
    comments_sig().update(|m| {
        if let Some(parent) = m.get_mut(&post_id).and_then(|t| t.items.iter_mut().find(|c| c.id == parent_id))
        {
            parent.replies.push(reply);
        }
    });
    bump_comment_count(post_id, 1);
}

/// Like / unlike a comment (or reply) by id.
pub fn toggle_comment_like(post_id: u64, comment_id: u64) {
    comments_sig().update(|m| {
        if let Some(t) = m.get_mut(&post_id) {
            t.items.iter_mut().any(|c| toggle_like_in(c, comment_id));
        }
    });
}

/// Delete one of the current user's own posts (guards against deleting others').
pub fn delete_post(id: u64) {
    let mine = post(id).is_some_and(|p| p.author == ME);
    if !mine {
        return;
    }
    feed_sig().update(|v| v.retain(|p| p.id != id));
    comments_sig().update(|m| {
        m.remove(&id);
    });
    if post_open() == Some(id) {
        close_post();
    }
    toast("Post deleted").show();
}

/// Follow / unfollow another user (updates their follower count + our button).
pub fn toggle_follow(id: u64) {
    if id == ME {
        return;
    }
    edit_user(id, |u| {
        u.i_follow = !u.i_follow;
        if u.i_follow {
            u.followers += 1;
        } else {
            u.followers = u.followers.saturating_sub(1);
        }
    });
}

/// Edit the current user's name + bio (from the profile sheet).
pub fn update_profile(name: &str, bio: &str) {
    seed_users_sig().update(|v| {
        if let Some(u) = v.iter_mut().find(|u| u.id == ME) {
            if !name.trim().is_empty() {
                u.name = name.trim().to_string();
            }
            u.bio = bio.trim().to_string();
        }
    });
    toast("Profile updated").show();
}

/// Mark every notification read (clears the badge).
pub fn mark_all_read() {
    notifs().update(|v| v.iter_mut().for_each(|n| n.read = true));
}

// --- post-detail navigation ------------------------------------------------

/// Open a post full-screen (the detail view with its comment thread).
pub fn open_post(id: u64) {
    post_route().set(Some(id));
}
/// Close the detail view, back to the feed.
pub fn close_post() {
    post_route().set(None);
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

/// Mutate one post in place. Returns the post's `bookmarked` flag *after* the edit,
/// which `toggle_bookmark` uses to pick its "Saved"/"Removed" toast.
fn edit_post(id: u64, f: impl FnOnce(&mut Post)) -> bool {
    let mut flag = false;
    feed_sig().update(|v| {
        if let Some(p) = v.iter_mut().find(|p| p.id == id) {
            f(p);
            flag = p.bookmarked;
        }
    });
    flag
}

/// Mutate one user in place — in whichever population (seed or API) owns the id.
fn edit_user(id: u64, f: impl FnOnce(&mut User)) {
    if id >= API_UID {
        api_users().update(|m| {
            if let Some(u) = m.get_mut(&id) {
                f(u);
            }
        });
    } else {
        seed_users_sig().update(|v| {
            if let Some(u) = v.iter_mut().find(|u| u.id == id) {
                f(u);
            }
        });
    }
}

/// Set a post's known comment count (called when its thread finishes loading).
fn set_comment_count(post_id: u64, n: u32) {
    edit_post(post_id, |p| p.comment_count = Some(n));
}

/// Nudge a post's comment count (when you add a comment or reply).
fn bump_comment_count(post_id: u64, delta: u32) {
    edit_post(post_id, |p| p.comment_count = Some(p.comment_count.unwrap_or(0) + delta));
}

/// A fresh comment authored by the current user.
fn new_local_comment(text: &str) -> Comment {
    let id = NEXT_COMMENT_ID.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    Comment { id, author: ME, text: text.to_string(), likes: 0, liked: false, replies: Vec::new() }
}

/// Toggle a like on `c` or any of its replies, matching by id. Returns whether it hit.
fn toggle_like_in(c: &mut Comment, target: u64) -> bool {
    if c.id == target {
        c.liked = !c.liked;
        if c.liked {
            c.likes += 1;
        } else {
            c.likes = c.likes.saturating_sub(1);
        }
        return true;
    }
    c.replies.iter_mut().any(|r| toggle_like_in(r, target))
}

/// Map an API post into our model — the API body becomes the text, and every post
/// gets a deterministic photo so the feed is visually rich as it streams in.
fn map_post(p: &net::ApiPost) -> Post {
    let text = if p.body.is_empty() { p.title.clone() } else { p.body.clone() };
    Post {
        id: p.id,
        author: API_UID + p.user_id,
        text,
        media: Some(photo(&format!("post-{}", p.id))),
        likes: p.reactions.likes,
        liked: false,
        bookmarked: false,
        comment_count: None,
        time: String::new(),
    }
}

/// Map an API comment into our model (its author resolves via the user directory).
fn map_comment(c: &net::ApiComment) -> Comment {
    Comment {
        id: c.id,
        author: API_UID + c.user.id,
        text: c.body.clone(),
        likes: c.likes,
        liked: false,
        replies: Vec::new(),
    }
}

/// Register commenters into the user directory from their inlined data (name +
/// handle), with a deterministic placeholder avatar. Existing directory entries (with
/// real avatars) are kept; this only fills gaps.
fn register_comment_authors(list: &[net::ApiComment]) {
    api_users().update(|m| {
        for cm in list {
            let uid = API_UID + cm.user.id;
            m.entry(uid).or_insert_with(|| User {
                id: uid,
                name: if cm.user.full_name.is_empty() {
                    cm.user.username.clone()
                } else {
                    cm.user.full_name.clone()
                },
                handle: cm.user.username.clone(),
                avatar: avatar((cm.user.id % 70 + 1) as u32),
                bio: String::new(),
                followers: 0,
                following: 0,
                i_follow: false,
            });
        }
    });
}

/// Map an API user into our model.
fn map_user(u: net::ApiUser) -> User {
    let full = format!("{} {}", u.first_name, u.last_name);
    let name = full.trim();
    User {
        id: API_UID + u.id,
        name: if name.is_empty() { u.username.clone() } else { name.to_string() },
        handle: u.username,
        avatar: u.image,
        bio: String::new(),
        followers: 0,
        following: 0,
        i_follow: false,
    }
}

/// A placeholder for an author whose profile hasn't arrived yet.
fn loading_user(id: u64) -> User {
    User {
        id,
        name: "Someone".into(),
        handle: "loading".into(),
        avatar: avatar(1),
        bio: String::new(),
        followers: 0,
        following: 0,
        i_follow: false,
    }
}

// ---------------------------------------------------------------------------
// Seed data — the curated, always-present world
// ---------------------------------------------------------------------------

fn seed_users() -> Vec<User> {
    vec![
        User {
            id: 1,
            name: "You".into(),
            handle: "you".into(),
            avatar: avatar(12),
            bio: "Building apps with Pebbles 🦀".into(),
            followers: 128,
            following: 87,
            i_follow: false,
        },
        User {
            id: 2,
            name: "Ada Lovelace".into(),
            handle: "ada".into(),
            avatar: avatar(5),
            bio: "First programmer".into(),
            followers: 9400,
            following: 12,
            i_follow: true,
        },
        User {
            id: 3,
            name: "Grace Hopper".into(),
            handle: "grace".into(),
            avatar: avatar(9),
            bio: "Compilers & bugs".into(),
            followers: 7300,
            following: 40,
            i_follow: false,
        },
        User {
            id: 4,
            name: "Alan Turing".into(),
            handle: "alan".into(),
            avatar: avatar(13),
            bio: "Machines that think".into(),
            followers: 8800,
            following: 5,
            i_follow: false,
        },
        User {
            id: 5,
            name: "Lin Clark".into(),
            handle: "lin".into(),
            avatar: avatar(20),
            bio: "Explains code with cartoons".into(),
            followers: 5200,
            following: 210,
            i_follow: true,
        },
    ]
}

/// Your own posts — shown immediately (and in your profile) while the live feed loads
/// in beneath them.
fn seed_feed() -> Vec<Post> {
    vec![
        Post {
            id: LOCAL_POST_BASE,
            author: ME,
            text: "First post from my Pebbles demo 👋 The feed below streams in live from a real API — \
                   scroll and watch it page in."
                .into(),
            media: Some(photo("hello-pebbles")),
            likes: 24,
            liked: true,
            bookmarked: false,
            comment_count: Some(2),
            time: "1d".into(),
        },
        Post {
            id: LOCAL_POST_BASE + 1,
            author: ME,
            text: "Infinite scroll, comments, replies — all running on Pebbles' reactive signals.".into(),
            media: Some(photo("reactive")),
            likes: 51,
            liked: false,
            bookmarked: true,
            comment_count: Some(0),
            time: "3h".into(),
        },
    ]
}

/// A couple of comments on your first post, so its detail view isn't empty.
fn seed_comments() -> HashMap<u64, Thread> {
    let mut m = HashMap::new();
    m.insert(
        LOCAL_POST_BASE,
        Thread {
            state: LoadState::Loaded,
            items: vec![
                Comment {
                    id: 1,
                    author: 2,
                    text: "Love this! The reactivity feels great. 🔥".into(),
                    likes: 3,
                    liked: false,
                    replies: vec![Comment {
                        id: 2,
                        author: ME,
                        text: "Thanks Ada! Try tapping a post to see its thread.".into(),
                        likes: 1,
                        liked: false,
                        replies: Vec::new(),
                    }],
                },
                Comment {
                    id: 3,
                    author: 3,
                    text: "How's the scroll performance with a long feed?".into(),
                    likes: 1,
                    liked: false,
                    replies: Vec::new(),
                },
            ],
        },
    );
    m
}

fn seed_notifs() -> Vec<Notif> {
    vec![
        Notif { kind: NotifKind::Like, actor: 2, read: false, time: "8m".into() },
        Notif { kind: NotifKind::Follow, actor: 3, read: false, time: "30m".into() },
        Notif { kind: NotifKind::Comment, actor: 4, read: false, time: "1h".into() },
        Notif { kind: NotifKind::Like, actor: 5, read: true, time: "3h".into() },
        Notif { kind: NotifKind::Follow, actor: 4, read: true, time: "1d".into() },
    ]
}

// ===========================================================================
// Messaging — conversations + a tiny full-screen router
// ===========================================================================

#[derive(Clone)]
pub struct Message {
    pub from: u64,
    pub text: String,
    pub time: String,
}

#[derive(Clone)]
pub struct Conversation {
    pub id: u64,
    pub user: u64, // the other participant
    pub messages: Vec<Message>,
    pub unread: u32,
}

/// The messaging surface's route — a full-screen takeover with two levels.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MsgView {
    Closed,
    List,
    Thread(u64),
}

thread_local! {
    static CONVOS: RefCell<Option<Signal<Vec<Conversation>>>> = const { RefCell::new(None) };
    static MSG_VIEW: RefCell<Option<Signal<MsgView>>> = const { RefCell::new(None) };
}

pub fn convos() -> Signal<Vec<Conversation>> {
    CONVOS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_convos())))
}
fn msg_view() -> Signal<MsgView> {
    MSG_VIEW.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(MsgView::Closed)))
}

// --- reads ------------------------------------------------------------------

pub fn messages_view() -> MsgView {
    msg_view().get()
}
pub fn messages_open() -> bool {
    messages_view() != MsgView::Closed
}
pub fn conversations() -> Vec<Conversation> {
    convos().get()
}
pub fn convo(id: u64) -> Option<Conversation> {
    convos().get().into_iter().find(|c| c.id == id)
}
/// Total unread messages (drives the top-bar badge).
pub fn unread_messages() -> usize {
    convos().get().iter().map(|c| c.unread as usize).sum()
}

// --- navigation actions -----------------------------------------------------

pub fn open_messages() {
    msg_view().set(MsgView::List);
}
pub fn open_thread(id: u64) {
    // Opening a thread clears its unread count.
    convos().update(|v| {
        if let Some(c) = v.iter_mut().find(|c| c.id == id) {
            c.unread = 0;
        }
    });
    msg_view().set(MsgView::Thread(id));
}
/// Back: a thread returns to the list; the list closes messaging.
pub fn messages_back() {
    match messages_view() {
        MsgView::Thread(_) => msg_view().set(MsgView::List),
        _ => msg_view().set(MsgView::Closed),
    }
}

/// Send a message in a conversation, then fake a friendly reply (the illusion).
pub fn send_message(id: u64, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let reply = {
        const REPLIES: [&str; 5] =
            ["Nice! 🙌", "Haha for real", "Let's ship it 🚀", "Agreed 💯", "On it — talk soon!"];
        let n = convo(id).map(|c| c.messages.len()).unwrap_or(0);
        REPLIES[n % REPLIES.len()]
    };
    convos().update(|v| {
        if let Some(c) = v.iter_mut().find(|c| c.id == id) {
            c.messages.push(Message { from: ME, text: text.to_string(), time: "now".into() });
            c.messages.push(Message { from: c.user, text: reply.into(), time: "now".into() });
        }
    });
}

fn seed_convos() -> Vec<Conversation> {
    vec![
        Conversation {
            id: 1,
            user: 2,
            unread: 2,
            messages: vec![
                Message { from: 2, text: "Did you try the new sheet fix?".into(), time: "10:02".into() },
                Message { from: 1, text: "Yeah, taps stay put now 🎉".into(), time: "10:03".into() },
                Message { from: 2, text: "Perfect. Ship it.".into(), time: "10:04".into() },
                Message {
                    from: 2, text: "Also — the dog logo looks great 🐶".into(), time: "10:05".into()
                },
            ],
        },
        Conversation {
            id: 2,
            user: 3,
            unread: 0,
            messages: vec![
                Message { from: 1, text: "Compiler question for you 👀".into(), time: "Mon".into() },
                Message { from: 3, text: "Always. Fire away.".into(), time: "Mon".into() },
            ],
        },
        Conversation {
            id: 3,
            user: 5,
            unread: 1,
            messages: vec![Message { from: 5, text: "Loved your last post!".into(), time: "Sun".into() }],
        },
    ]
}
