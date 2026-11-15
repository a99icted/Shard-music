# Local network transfer protocol, version 2

Implementation: `src/sync.rs` and `src/sync/transfer.rs`. This is the foundation
for a future mobile client. Transfers start manually from a track or playlist
menu, or **Synchronize all tracks** in Settings → Synchronization or the library
header. One paired peer and one operation are supported per client at a time.
Both clients must support version 2; version 1 clients must be updated.

## Connecting

Turn on synchronization on both devices. Copy one device's code using the icon
beside its read-only code field, paste it into the other client's connection
field, and connect. The QR contains exactly the same UTF-8 payload:

```text
shard-sync://192.168.1.10:49152/xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
```

The port is allocated dynamically. The UUID in the path is a random session
secret. Enabling synchronization again rotates the port and secret; peers must
reconnect. The stable device UUID is stored in preferences. The desktop displays
the QR for a future mobile scanner; it does not use a camera.

Only private, shared (RFC 6598), loopback and link-local IP addresses are accepted.
The address comes from the system route, with an interface fallback when a VPN
hides it. Both devices must be reachable; private-network firewall access may
need to be allowed. VPNs and multiple adapters can affect routing.

Transport is TCP with session-secret authentication and SHA-256 integrity checks.
It is **not encrypted** and is intended for a trusted LAN. TLS with pinned device
identities should precede use on untrusted networks. Codes grant pairing access.

## Frames and pairing

Each JSON message is a four-byte unsigned big-endian length followed by that many
UTF-8 JSON bytes. Frames are limited to 8 MiB. Socket read/write timeouts are
30 seconds; the sender allows 600 seconds for inspection, preparation and commit.
A connection handles one request.

Disabling synchronization immediately drops the listening socket, shuts down
registered incoming/outgoing connections and wakes the listener worker. Windows
can leave a receive pending after shutdown on a duplicate socket handle, so
established connections use 100 ms OS I/O timeouts with cancellation checks while
preserving the logical deadlines above. Partial frames are retained across these
short waits; disabling does not wait for the 30/600 second network deadline.
Incomplete audio caches remain available for resume after reconnecting.

```json
{
  "type": "pair",
  "version": 2,
  "token": "target-session-uuid",
  "peer": {
    "code": "shard-sync://192.168.1.11:49153/initiator-session-uuid",
    "name": "Laptop",
    "device": "stable-initiator-device-uuid"
  }
}
```

The receiver verifies version, token, device UUID and initiator IP, and returns
`ok: true` with its own `peer`. Both devices now know the other's listener and
stable identity, so either can send. Inspection and transfer requests require
the target token, paired source IP and matching manifest device UUID. Failures
return `ok: false` with an `error` string. Irrelevant reply fields serialize with
empty/default values.

## Manifest and duplicate inspection

The sender hashes selected tracks and persists an operation UUID, keyed by peer
and selection scope. Retrying an unchanged selection reuses that UUID until a
final success acknowledgement. A manifest fingerprint excludes the operation UUID
and includes all other manifest fields. First send an `inspect` request:

```json
{
  "type": "inspect",
  "token": "target-session-uuid",
  "manifest": {
    "version": 2,
    "transfer_id": "operation-uuid",
    "device": "stable-sender-device-uuid",
    "playlist": "42",
    "title": "Evening music",
    "favorites": false,
    "library": false,
    "tracks": [{
      "source_id": "17",
      "title": "Song",
      "artist": "Artist",
      "source_url": null,
      "duration_ms": 180000,
      "extension": "mp3",
      "hash": "64-lowercase-hex-sha256-characters",
      "size": 5000000,
      "favorite": true
    }]
  }
}
```

`source_id` is the stable sender-local track ID. A standalone track has
`playlist: null`, `library: false`. Favorites has `playlist: "-1"`, `favorites:
true`. The entire library has `playlist: null`, `library: true`, and creates no
receiving playlist. Track order is manifest order. Extensions match
`AUDIO_EXTENSIONS` in `src/services.rs`. Limits: 10,000 tracks, 2 GiB per file,
32 GiB per operation. An empty playlist can be sent.

The receiver checks its entire library, including local imports, and earlier
manifest entries. Match priority: identical SHA-256, same nonempty source URL,
then normalized title and artist with known durations within three seconds.
Metadata matches can represent different recordings; the UI identifies these
as possible matches. File hashes are cached by track ID, size and modification
time. An inspection reply includes:

```json
{
  "ok": true,
  "done": false,
  "duplicates": [{
    "index": 0,
    "title": "Song",
    "artist": "Artist",
    "existing_title": "Song",
    "kind": "IdenticalFile"
  }]
}
```

Kinds: `IdenticalFile`, `SameSource`, `SimilarMetadata`. Previously synchronized
source tracks in a playlist/library are reused automatically, rather than
reported again. Source device, source track ID and incoming hash identify the
origin. Standalone transfers still permit an explicit copy. Already committed
operation IDs return `done: true` without duplicate questions.

## User choices

Playlist and entire-library transfers open an options dialog before starting.
Preferences persist, with these defaults:

| Warn about duplicates | Ignore duplicates | Behavior |
| --- | --- | --- |
| On (default) | Off (default) | Ask whether to skip/reuse or send a copy. |
| On | On | Ask, with skip/reuse emphasized. |
| Off | On | Reuse matches without asking. |
| Off | Off | Create additional copies for detected matches. |

The duplicate dialog supports a decision for each match or one choice for all
remaining matches. Closing/cancelling cancels the operation. Standalone tracks
always ask about matches. No socket or SQLite transaction remains open while
waiting for a choice. The circled question mark explains the options; closing
help returns to the options dialog.

## Transfer and resume

Send the same manifest with `type: "transfer"`, target `token`, and `copies`: an
array of manifest indexes explicitly requested as additional copies. Other
matches reuse their local or earlier-manifest track. The receiver replans against
its current library and returns:

```json
{"ok": true, "done": false, "missing": [0, 2], "offsets": [0, 50000]}
```

`offsets` aligns with `missing`. For each requested index, in order, the sender
seeks to its offset and writes exactly `size - offset` **raw audio bytes**, without
extra framing. Fully cached files are omitted. Both clients display byte progress
for requested files, including saved partial offsets.

The receiver appends to `sync-cache/{hash}.{extension}.part`. Completed files are
SHA-256 checked and parsed as audio, then renamed to `{hash}.{extension}`. Partial
and complete cache files survive failures and restarts. Repeating an operation
sends only missing files and the remainder of an interrupted file. Bad hashes
remove the corrupt partial so another attempt can resend it.

After receiving audio, the receiver commits tracks, playlist memberships,
favorites, source mappings and a completed-operation receipt in one SQLite
transaction. Failed transactions remove newly created library audio files and
retain cache for retry. The visible library remains unchanged on failure.
After commit the cache is cleaned; the final reply contains:

```json
{"ok": true, "report": {"copied": 2, "reused": 4, "resumed_bytes": 50000}}
```

`resumed_bytes` counts saved offsets for requested partial files. The sender marks
completion only after this reply. If acknowledgement is lost, replaying the same
operation UUID is acknowledged without creating copies, even when `copies` was
selected. Reusing an operation UUID with a different fingerprint is rejected.

## Merge rules and scope

- Sender device UUID plus playlist ID identifies the receiving playlist. First
  transfer creates it; later transfers append missing memberships. Existing
  playlist names, metadata and ordering are retained.
- Repeating a growing playlist/library sends new tracks and reuses prior origins,
  even when duplicate options otherwise permit additional copies.
- Favorites merge into built-in Favorites. Individual tracks carry favorite
  marks too. Sending the library does not create a new playlist.
- Tracks, memberships and favorite marks are never removed. This is manual
  additive transfer rather than automatic state mirroring.
- New tracks preserve titles, artists, source URLs, duration and order. Covers,
  backgrounds, playback state and settings are not transferred.
- Simultaneous peers, persistent pairing, encrypted transport and a mobile QR
  scanner remain future work. Abandoned resume caches have no automatic expiry.
