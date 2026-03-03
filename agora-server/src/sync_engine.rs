use std::collections::HashMap;
use std::sync::Mutex;

use agora_core::events::RoomEvent;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 256;

/// Lightweight in-memory broadcast layer for real-time event delivery.
///
/// Each room gets a `tokio::sync::broadcast` channel. When an event is
/// persisted, it is also broadcast here so that long-polling `/sync`
/// requests wake up immediately.
pub struct SyncEngine {
    /// Map from room_id -> broadcast sender.
    rooms: Mutex<HashMap<String, broadcast::Sender<SyncEvent>>>,
}

#[derive(Debug, Clone)]
pub struct SyncEvent {
    pub event: RoomEvent,
    pub stream_ordering: i64,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            rooms: Mutex::new(HashMap::new()),
        }
    }

    /// Broadcast an event to all listeners on a room channel.
    pub fn broadcast(&self, room_id: &str, event: &RoomEvent, stream_ordering: i64) {
        let mut rooms = self.rooms.lock().unwrap();
        let tx = rooms
            .entry(room_id.to_owned())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0);

        // It's fine if nobody is listening — the send just drops.
        let _ = tx.send(SyncEvent {
            event: event.clone(),
            stream_ordering,
        });
    }

    /// Subscribe to a room's broadcast channel. Returns a receiver.
    pub fn subscribe(&self, room_id: &str) -> broadcast::Receiver<SyncEvent> {
        let mut rooms = self.rooms.lock().unwrap();
        let tx = rooms
            .entry(room_id.to_owned())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0);
        tx.subscribe()
    }
}
