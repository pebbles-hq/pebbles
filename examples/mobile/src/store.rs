//! The **app state manager** — the whole "backend", in memory.
//!
//! Everything the UI shows lives in a handful of app-scoped signals (users, posts,
//! notifications). Every interaction goes through an **action** here; components only
//! read the signals (and re-render when they change) and call actions. There is no
//! server — the illusion of a working app is entirely this file. Swap these functions
//! for real HTTP calls later and the UI wouldn't change.

use std::cell::{Cell, RefCell};

use pebbles::prelude::*;

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

#[derive(Clone)]
pub struct Comment {
    pub author: u64,
    pub text: String,
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
    pub comments: Vec<Comment>,
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

/// The signed-in user's id (fixed — this is a demo).
pub const ME: u64 = 1;

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
// State — three app-scoped signals + a couple of counters
// ---------------------------------------------------------------------------

thread_local! {
    static USERS: RefCell<Option<Signal<Vec<User>>>> = const { RefCell::new(None) };
    static POSTS: RefCell<Option<Signal<Vec<Post>>>> = const { RefCell::new(None) };
    static NOTIFS: RefCell<Option<Signal<Vec<Notif>>>> = const { RefCell::new(None) };
    static NEXT_POST_ID: Cell<u64> = const { Cell::new(100) };
}

pub fn users() -> Signal<Vec<User>> {
    USERS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_users())))
}
pub fn posts() -> Signal<Vec<Post>> {
    POSTS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_posts())))
}
pub fn notifs() -> Signal<Vec<Notif>> {
    NOTIFS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_notifs())))
}

// ---------------------------------------------------------------------------
// Reads / lookups
// ---------------------------------------------------------------------------

/// Look up a user by id (clones — fine for a demo).
pub fn user(id: u64) -> User {
    users().get().into_iter().find(|u| u.id == id).unwrap_or_else(|| unknown(id))
}

pub fn me() -> User {
    user(ME)
}

/// The feed — every post, newest first (they're stored newest-first already).
pub fn feed() -> Vec<Post> {
    posts().get()
}

/// The current user's own posts (for the profile grid).
pub fn my_posts() -> Vec<Post> {
    posts().get().into_iter().filter(|p| p.author == ME).collect()
}

/// A single post by id.
pub fn post(id: u64) -> Option<Post> {
    posts().get().into_iter().find(|p| p.id == id)
}

/// Unread notification count (drives the tab badge).
pub fn unread() -> usize {
    notifs().get().iter().filter(|n| !n.read).count()
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
        comments: Vec::new(),
        time: "now".into(),
    };
    posts().update(|v| v.insert(0, post));
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

/// Add a comment to a post.
pub fn add_comment(id: u64, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    edit_post(id, |p| p.comments.push(Comment { author: ME, text: text.to_string() }));
}

/// Delete one of the current user's own posts (guards against deleting others').
pub fn delete_post(id: u64) {
    let mine = post(id).is_some_and(|p| p.author == ME);
    if !mine {
        return;
    }
    posts().update(|v| v.retain(|p| p.id != id));
    toast("Post deleted").show();
}

/// Follow / unfollow another user (updates their follower count + our button).
pub fn toggle_follow(id: u64) {
    if id == ME {
        return;
    }
    users().update(|v| {
        if let Some(u) = v.iter_mut().find(|u| u.id == id) {
            u.i_follow = !u.i_follow;
            if u.i_follow {
                u.followers += 1;
            } else {
                u.followers = u.followers.saturating_sub(1);
            }
        }
    });
}

/// Edit the current user's name + bio (from the profile sheet).
pub fn update_profile(name: &str, bio: &str) {
    users().update(|v| {
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

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

/// Mutate one post in place. Returns the post's `bookmarked` flag *after* the edit,
/// which `toggle_bookmark` uses to pick its "Saved"/"Removed" toast.
fn edit_post(id: u64, f: impl FnOnce(&mut Post)) -> bool {
    let mut flag = false;
    posts().update(|v| {
        if let Some(p) = v.iter_mut().find(|p| p.id == id) {
            f(p);
            flag = p.bookmarked;
        }
    });
    flag
}

fn unknown(id: u64) -> User {
    User {
        id,
        name: "Unknown".into(),
        handle: "unknown".into(),
        avatar: avatar(1),
        bio: String::new(),
        followers: 0,
        following: 0,
        i_follow: false,
    }
}

// ---------------------------------------------------------------------------
// Seed data
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

fn seed_posts() -> Vec<Post> {
    vec![
        Post {
            id: 5,
            author: 2,
            text: "Signals instead of setState — the whole app is a function returning a widget.".into(),
            media: Some(photo("mountains")),
            likes: 342,
            liked: false,
            bookmarked: false,
            comments: vec![Comment { author: 3, text: "This is the way.".into() }],
            time: "12m".into(),
        },
        Post {
            id: 4,
            author: 3,
            text: "Bottom nav, a scaffold, a FAB — a real mobile shell, drawn on the GPU via Vello.".into(),
            media: Some(photo("city")),
            likes: 210,
            liked: true,
            bookmarked: true,
            comments: vec![],
            time: "48m".into(),
        },
        Post {
            id: 3,
            author: 5,
            text: "TIL the entire UI here is one reactive graph. No virtual DOM diffing.".into(),
            media: None,
            likes: 96,
            liked: false,
            bookmarked: false,
            comments: vec![
                Comment { author: 2, text: "Wild.".into() },
                Comment { author: 4, text: "How's the perf?".into() },
            ],
            time: "2h".into(),
        },
        Post {
            id: 2,
            author: 4,
            text: "Shipping a GUI in Rust that feels like Flutter. Wild how little ceremony there is.".into(),
            media: Some(photo("desk")),
            likes: 501,
            liked: false,
            bookmarked: false,
            comments: vec![],
            time: "5h".into(),
        },
        Post {
            id: 1,
            author: 1,
            text: "First post from my Pebbles demo app 👋".into(),
            media: Some(photo("coffee")),
            likes: 24,
            liked: true,
            bookmarked: false,
            comments: vec![],
            time: "1d".into(),
        },
    ]
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
