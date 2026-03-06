use std::collections::BTreeMap;
use std::sync::Mutex;

use agora_core::events::presence::PresenceEvent;
use agora_core::events::RoomEvent;
use tokio::sync::broadcast;

const CHANNEL_CAPACITY: usize = 256;

/// Presence update event for real-time delivery.
#[derive(Debug, Clone)]
pub struct PresenceUpdate {
    pub presence: PresenceEvent,
    pub room_id: String,
}

/// Lightweight in-memory broadcast layer for real-time event delivery.
///
/// Each room gets a `tokio::sync::broadcast` channel. When an event is
/// persisted, it is also broadcast here so that long-polling `/sync`
/// requests wake up immediately.
pub struct SyncEngine {
    /// Map from room_id -> broadcast sender.
    rooms: Mutex<BTreeMap<String, broadcast::Sender<SyncEvent>>>,
    /// Presence updates channel for real-time presence delivery.
    presence_tx: broadcast::Sender<PresenceUpdate>,
}

#[derive(Debug, Clone)]
pub struct SyncEvent {
    pub event: RoomEvent,
    pub stream_ordering: i64,
}

impl SyncEngine {
    pub fn new() -> Self {
        let (presence_tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            rooms: Mutex::new(BTreeMap::new()),
            presence_tx,
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

    /// Broadcast a presence update to all listeners.
    pub fn broadcast_presence(&self, room_id: &str, presence: PresenceEvent) {
        let _ = self.presence_tx.send(PresenceUpdate {
            presence,
            room_id: room_id.to_owned(),
        });
    }

    /// Subscribe to presence updates.
    pub fn subscribe_presence(&self) -> broadcast::Receiver<PresenceUpdate> {
        self.presence_tx.subscribe()
    }
}
