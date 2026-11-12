# Changelog

## 2.0.0

- Builds against libopus 1.6.1 using the published opusic-sys binding.
- Requires Node.js 22.22.2 or newer.
- Reads and writes aligned PCM16LE buffers directly through the Into methods.
  Unaligned buffers use reusable scratch storage.
- Checks decode capacity before consuming the packet, so callers can retry with
  a larger destination without losing decoder state.
- Rejects overlapping input/output buffers and CTL requests that require pointer
  arguments. Integer setters and reset remain available through the CTL methods.
- Uses Symphonia 0.6.1 for probing. Async probing snapshots input before worker
  dispatch; synchronous probing borrows the input during the call.

The constructor, factory, allocating methods, Into methods, encoder control
aliases, version exports and probe exports remain available.
