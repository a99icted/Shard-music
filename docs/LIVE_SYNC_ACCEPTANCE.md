# Live sync: shared desktop / Android acceptance contract

Status: planned v3 acceptance scenarios, not a claim that live sync is implemented.
Product semantics and proposed storage/protocol are in
[LIVE_SYNC_DESIGN.ru.md](LIVE_SYNC_DESIGN.ru.md). Current v2 is in [LAN_SYNC.md](LAN_SYNC.md).

Every applicable case must run desktop→Android and Android→desktop. Use temporary
libraries, generated audio and fixed device/entity/event IDs. Add JSON golden
fixtures when implementing v3; enum names, null handling, integer millisecond
positions and UUID representation must match across language bindings.

## Delivered v2 device-list step

- [x] Settings tab is conditional on synchronization being enabled.
- [x] Empty state links to pairing settings.
- [x] Actual paired name, device UUID, endpoint without session secret, protocol version.
- [x] Manual transfer action uses the real existing sync flow.
- [x] Status says session-paired, without claiming heartbeat-backed online presence.
- [x] Local stop clears paired status; restart starts without a paired device.
- [x] Real two-service test asserts both identities, endpoint redaction, stop/restart.

## Identity, connection and permissions (v3)

- [ ] First pairing establishes identity; restart reconnects to that identity, not just IP.
- [ ] Device rename/address change does not create another profile or lose cursors.
- [ ] Two devices with the same display name remain distinguishable.
- [ ] Multiple devices pair without replacing the existing list.
- [ ] Offline/online is derived from authenticated channel/heartbeat.
- [ ] Disconnect closes the session; forget/revoke prevents automatic reconnection.
- [ ] Old v2 device offers manual transfer only; cannot silently accept live policies.
- [ ] A sender cannot enable actions/deletions/statistics on a receiver unilaterally.
- [ ] Policy changes and disabled categories cannot block unrelated cursors forever.
- [ ] No automatic transitive sharing between separately trusted device pairs.

## Playback

- [ ] `off`: phone and desktop play independently; data synchronization may continue.
- [ ] `host_only`: phone play/pause/seek/next controls desktop; phone emits no audio.
- [ ] Desktop transport and phone remote panel converge on the same canonical state.
- [ ] `all`: explicit play on phone plays only desktop; explicit play on desktop plays only phone.
- [ ] Initiator emits no local audio for its remote play; panel identifies the actual renderer.
- [ ] Received play command, automatic next and repeat never route playback back to sender.
- [ ] Pause/resume/seek/next from either device control the current renderer without changing target.
- [ ] New explicit track selection resolves the other device as target, even if initiated on the current renderer.
- [ ] Switching renderer prepares new audio, acknowledges the previous renderer's stop, then starts only the target.
- [ ] Failed/unknown stop acknowledgement or offline target never starts an automatic local fallback.
- [ ] With more than two devices, an explicit remote target is selected; own device is excluded and audio is not broadcast.
- [ ] Simultaneous commands are serialized by host; stale revisions return snapshot.
- [ ] Replayed `next` command does not skip twice; remote state produces no command loop.
- [ ] Expired command / previous session command is rejected after reconnect.
- [ ] Local volume does not unexpectedly change volume on other renderers.
- [ ] Playback can be controlled during a large audio transfer.
- [ ] Host loss is visible and does not silently elect another host/start another device.
- [ ] Explicit play of ignored track works; automatic queue/autoplay never selects it.
- [ ] New ignore on current track skips immediately exactly once, including remote event replay.
- [ ] Entering live or adding a device does not start audio without a deliberate join/play.
- [ ] `all` preparation has cancellation/timeout; a newer play supersedes pending preparation.

## Catalog / audio

- [ ] Metadata-only exchange creates a phantom without invalid local audio paths.
- [ ] An entity available on two peers appears once with both sources.
- [ ] Same title/different recording is not merged automatically.
- [ ] Phantom can belong to a playlist without downloading audio.
- [ ] Offline peer keeps cached metadata and shows source unavailable.
- [ ] Partial/aborted snapshot never removes entries from unseen pages.
- [ ] Download-on-demand verifies hash, updates availability and starts chosen track.
- [ ] Resume/cancel/low-space preserve already committed library data.
- [ ] Local cache eviction does not delete remote music or playlist membership.
- [ ] All-tracks mirroring respects explicit local eviction override.
- [ ] All-tracks downloads obey network/storage limits and do not starve control messages.

## Library actions and flags

- [ ] Local mutation and event outbox are atomic; crash between them cannot lose intent.
- [ ] Apply+receipt is atomic; repeated delivery is a no-op.
- [ ] Offline concurrent metadata edits converge, without overwriting unrelated fields.
- [ ] Concurrent playlist add/remove/reorder follows the documented deterministic rule.
- [ ] Favorite on/off and ignore on/off obey per-direction category permissions.
- [ ] Applying ignore atomically clears favorite; clearing ignore never restores favorite.
- [ ] Disabled favorite sync does not disable the local ignore⇒unfavorite invariant.
- [ ] Entity deletion leaves tombstone; delayed old snapshots/events cannot resurrect it.
- [ ] Enabling delete sync does not silently replay historical deletes before permission.
- [ ] Clearing local storage / delete-all-ignored shows explicit local vs remote scope.
- [ ] Offline device returning after tombstone GC must bootstrap a new epoch.

## Statistics

- [ ] Renderer records listening; remote controller records no duplicate playback.
- [ ] Replayed/reordered segment batches do not increase totals twice.
- [ ] Pause and seek do not count as heard time; natural completion does.
- [ ] `all` records only on the remote renderer; initiating controller creates no duplicate listen.
- [ ] Renderer switch closes the old device's segment and opens the new one's without replaying totals.
- [ ] Per-device totals still show actual listening on that device.
- [ ] Device include/exclude changes only local summary, retaining underlying history.
- [ ] Deleted tracks keep historical metadata/time through UID/snapshot.
- [ ] Legacy totals are migrated once with provenance; no invented detailed listens.
- [ ] Midnight/timezone change and offline history use documented calendar rules.
- [ ] Turning history sync off stops transfer; re-enable offers historical backfill explicitly.

## Android and rollout

- [ ] Actual player uses Media3 session/service and respects audio focus/headset disconnect.
- [ ] Remote-only controller does not misuse a fake playing foreground media service.
- [ ] Screen off, process recreation, Wi-Fi change and app update reconnect safely.
- [ ] Database migrations are exercised on old desktop and Android libraries.
- [ ] Unsupported capabilities disable corresponding controls with an explanation.
- [ ] Both clients pass the same protocol fixtures before a capability is advertised.
