# Agora Roadmap

## Current State

Agora implements a substantial subset of the Matrix Client-Server API (v1.11) covering authentication, rooms, spaces, messaging, media, E2E encryption (Olm/Megolm), profiles, typing, devices, room directory, and more. The server is designed as a single self-hosted binary with SQLite storage.

## Remaining Items for Element/Standard Client Compatibility

The following features would be needed for full compatibility with standard Matrix clients like Element, FluffyChat, etc.

### Filters

- `POST /user/{userId}/filter` — create server-side filter
- `GET /user/{userId}/filter/{filterId}` — retrieve filter
- `filter` parameter on `/sync` — apply filter to sync response
- `filter` parameter on `/messages` — apply RoomEventFilter

### Room Moderation

- `POST /rooms/{roomId}/kick` — kick a user from a room
- `POST /rooms/{roomId}/ban` — ban a user from a room
- `POST /rooms/{roomId}/unban` — unban a user
- Power level enforcement on all state-changing operations
- `m.room.power_levels` state event processing and validation

### Presence

- `PUT /presence/{userId}/status` — set presence status (online/offline/unavailable)
- `GET /presence/{userId}/status` — get presence status
- `set_presence` query parameter on `/sync`
- `presence` section in sync response with `m.presence` events
- Automatic idle/offline detection based on inactivity

### Read Receipts

- `POST /rooms/{roomId}/receipt/{receiptType}/{eventId}` — send read receipt
- Include receipts in `ephemeral.events` section of sync response
- `m.read` and `m.read.private` receipt types
- Fully-read markers (`m.fully_read` account data)

### Account Data

- `PUT /user/{userId}/account_data/{type}` — set global account data
- `GET /user/{userId}/account_data/{type}` — get global account data
- `PUT /user/{userId}/rooms/{roomId}/account_data/{type}` — per-room account data
- `GET /user/{userId}/rooms/{roomId}/account_data/{type}` — get per-room account data
- Include `account_data` section in sync response (global and per-room)

### Room Versioning

- Room version field in `m.room.create` content
- `POST /rooms/{roomId}/upgrade` — upgrade room to a new version
- Proper handling of room version capabilities

### E2E Encryption Enhancements

- `device_lists` section in sync response (changed/left device tracking)
- Device list change notifications when users join/leave rooms
- Cross-signing (master/self-signing/user-signing keys)
- Key backup (`/room_keys/` endpoints)
- Key sharing (`m.room_key_request`, `m.forwarded_room_key`)
- Encrypted attachments (AES-CTR file encryption)
- Verification (SAS, QR code)

### Federation (Server-to-Server API)

- `/_matrix/federation/v1/` endpoints
- Server key management and signing
- Event authorization and state resolution
- Backfill from remote servers
- Room joins via federation

### Authentication Enhancements

- User-Interactive Authentication (UIAA) for `/register` and sensitive operations
- `m.login.token` login type
- SSO/OAuth2 login flow
- Refresh tokens (`refresh_token` in login response)
- Password change (`POST /account/password`)

### Push Notifications

- `GET /pushers` — list push notification targets
- `POST /set/pusher` — configure push notifications
- `GET /pushrules/` — get push rules
- `PUT /pushrules/` — set push rules
- Push gateway integration

### Content Repository Enhancements

- `GET /media/v3/thumbnail/{serverName}/{mediaId}` — thumbnailing
- Content-Type validation
- Media quarantine
- URL previews (`GET /media/v3/preview_url`)

### Room Alias Resolution in /join

- Resolve `#alias:server` to room_id in the `/join/{roomIdOrAlias}` endpoint
- Support server_name parameter for joining via alias

### Search

- `POST /search` — full-text message search
- Search by room, sender, and content

### Third-Party Networks

- `GET /thirdparty/protocols` — list bridged protocols
- Application service (bridge) registration

## Feature Priorities

1. **High** — Filters, moderation (kick/ban), read receipts, account data
2. **Medium** — Presence, E2E enhancements (cross-signing, key backup), search
3. **Low** — Federation, push notifications, SSO, third-party networks
