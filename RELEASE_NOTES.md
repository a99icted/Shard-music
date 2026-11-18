# Shard Windows 0.1.5 / Android 0.1.7 — Hotspot Synchronization

Connecting through a phone or PC hotspot could fail even when router Wi-Fi
worked. A PC could put its internet uplink address in the QR instead of its
hotspot address. A phone sharing Wi-Fi could advertise its mobile-data address
and receive “Invalid device address” from the PC.

The QR now includes up to eight local addresses. The connecting app prefers its
own subnet and tries other addresses when a TCP connection fails. Both devices
use the established connection's interface for their return address, so either
can send music after pairing.

Update **both apps**: Windows **0.1.5** and Android **0.1.7**. Older apps do not
understand QR codes containing multiple addresses; single-address codes remain
valid. Transfer protocol version 2, session credentials and source-IP checks are
preserved.

Enable the hotspot and connect the second device before enabling synchronization.
After switching networks, restart synchronization on both devices and scan the
new PC QR. VPNs must allow LAN access; Windows must allow Shard on the local
network.

The automated TCP regression test models an unavailable first QR address and
incorrect default interface addresses on both devices, then verifies pairing
and audio transfers in both directions. Android also has a JNI instrumentation
test for QR fallback and the advertised socket address. Physical phone/PC
hotspot combinations still require testing on the user's devices.

Windows: `Shard-0.1.5-Setup-x64.exe`. Android: `Shard-Android-0.1.7-arm64.apk`
for a modern physical phone, or `Shard-Android-0.1.7-x86_64.apk` for an emulator.
