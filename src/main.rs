mod messages;

// #![deny(warnings)]
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use tokio::sync::{mpsc, RwLock};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};
use warp::Filter;
use crate::messages::{IncomingMessage, OutgoingMessage};

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

/// Our state of currently connected users.
///
/// - Key is their id
/// - Value is a sender of `warp::ws::Message`
type Users = Arc<RwLock<HashMap<usize, User>>>;

struct User {
    username: String,
    tx: UnboundedSender<Message>,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Keep track of all connected users, key is usize, value
    // is a websocket sender.
    let users = Users::default();
    // Turn our "state" into a new Filter...
    let users = warp::any().map(move || users.clone());

    // GET /ws -> websocket upgrade
    let chat = warp::path("ws")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(users)
        .and(warp::header("X-Forwarded-User"))
        .map(|ws: warp::ws::Ws, users, username: String| {
            // This will call our function if the handshake succeeds.
            ws.on_upgrade(move |socket| user_connected(socket, users, username))
        });

    // GET / -> index html
    let index = warp::path::end().map(|| warp::reply::html("<html><body><h1>hello</h1></body></html>"));

    let routes = index.or(chat);

    warp::serve(routes).run(([0, 0, 0, 0], 8080)).await;
}

async fn user_connected(ws: WebSocket, users: Users, username: String) {
    // Use a counter to assign a new unique ID for this user.
    let my_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    eprintln!("new chat user: {}", &username);

    // Split the socket into a sender and receive of messages.
    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            user_ws_tx
                .send(message)
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                })
                .await;
        }
    });

    // Save the sender in our list of connected users.
    users.write().await.insert(my_id, User { username: username.clone(), tx });

    // Return a `Future` that is basically a state machine managing
    // this specific user's connection.

    // Every time the user sends a message, broadcast it to
    // all other users...
    while let Some(result) = user_ws_rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("websocket error(uid={}, username={}): {}", my_id, &username, e);
                break;
            }
        };
        user_message(my_id, msg, &users, &username).await;
    }

    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    user_disconnected(my_id, &users).await;
}

async fn user_message(my_id: usize, msg: Message, users: &Users, username: &str) -> anyhow::Result<()> {
    let incoming = serde_json::from_str::<IncomingMessage>(msg.to_str()?)?;

    match incoming {
        IncomingMessage::Song { url } => {
            send_all(serde_json::to_string(&OutgoingMessage::Pong)?, users);
        },
        IncomingMessage::Volume { volume } => {
            send_all(serde_json::to_string(&OutgoingMessage::Pong)?, users);
        },
        IncomingMessage::Skip => {
            send_all(serde_json::to_string(&OutgoingMessage::Pong)?, users);
        },
        IncomingMessage::Ping => {
            // TODO: Get rid of useless hashmap lookup, just pass this from prev function..
            if let Some(tx) = users.read().await.get(&my_id) {
                tx.send(Message::text(serde_json::to_string(&OutgoingMessage::Pong)))
            }
        },
    };

    Ok(())
}

async fn send_all(message: String, users: &Users) {
    for (&uid, tx) in users.read().await.iter() {
        if let Err(_disconnected) = tx.send(Message::text(&message)) {
            // The tx is disconnected, our `user_disconnected` code
            // should be happening in another task, nothing more to
            // do here.
        }
    }
}

async fn user_disconnected(my_id: usize, users: &Users) {
    // TODO: Logging/tracing/etc..
    users.write().await.remove(&my_id);
}
