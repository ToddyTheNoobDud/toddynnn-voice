# @toddynnn/voice-opus

Opus encoding and decoding for Node.js, backed by libopus 1.6.1. The package
accepts PCM16LE audio and includes synchronous methods that write into buffers
you provide. It also exposes media probing through Symphonia.

## Install

```sh
npm install @toddynnn/voice-opus
```

Node.js 22.22.2 or newer is required. The matching native binary is installed as
an optional platform package, so keep optional dependencies enabled. Rust and a
C compiler are only needed when building from source.

## Encode and decode

```js
const { OpusEncoder, getOpusVersion } = require('@toddynnn/voice-opus')

const codec = new OpusEncoder(48000, 2, 'audio')
const pcm = Buffer.alloc(3840) // 20 ms of stereo PCM16LE at 48 kHz
const packet = codec.encode(pcm)
const decoded = codec.decode(packet)

console.log(getOpusVersion()) // libopus 1.6.1
console.log(decoded.length) // 3840 bytes
```

ES modules can use named imports:

```js
import { OpusEncoder, getOpusVersion } from '@toddynnn/voice-opus'
```

Each instance holds separate encoder and decoder state. Use a separate instance
for each independent audio stream.

## Reuse output buffers

`encodeInto` and `decodeInto` return the number of bytes written. Only that
prefix of the destination is valid output.

```js
const { OpusEncoder } = require('@toddynnn/voice-opus')

const codec = new OpusEncoder(48000, 2, 'audio')
const pcmInput = Buffer.alloc(3840)
const packetOutput = Buffer.alloc(4000)
const pcmOutput = Buffer.alloc(23040)

const packetBytes = codec.encodeInto(pcmInput, packetOutput)
const packet = packetOutput.subarray(0, packetBytes)
const pcmBytes = codec.decodeInto(packet, pcmOutput)
const decoded = pcmOutput.subarray(0, pcmBytes)
```

Reuse these buffers after their contents have been consumed. A subarray shares
its backing memory, so the next call can overwrite it. Copy a result if it must
stay valid across calls:

```js
const savedPacket = Buffer.from(packet)
```

The Into methods reuse native scratch and caller-owned output storage. Aligned
PCM16LE input and output take the direct native path on little-endian systems.
Unaligned views use preallocated scratch. Constructors and the allocating
`encode`/`decode` methods allocate memory; creating JavaScript views or copies
also has its own cost.

Input and output must not overlap or be accessed concurrently. The methods
accept both Buffer and Uint8Array. Invalid PCM sizes, short decode destinations
and codec failures throw JavaScript errors. A short decode destination is
rejected before the packet advances decoder state.

## Audio format

PCM is signed 16-bit little-endian audio, interleaved when using two channels.
Frame sizes are measured per channel.

| Setting | Values |
|---|---|
| Sample rate | 8000, 12000, 16000, 24000 or 48000 Hz |
| Channels | 1 or 2 |
| Encode frame duration | 2.5, 5, 10, 20, 40 or 60 ms |
| Maximum decoded packet duration | 120 ms |
| Application | `audio`, `voip` or `lowdelay`; numeric values 2049, 2048 or 2051 |

At 48 kHz, a 20 ms frame contains 960 samples per channel. Stereo PCM therefore
uses `960 * 2 * 2 = 3840` bytes. A 23040-byte destination holds the longest
decoded packet at that rate and channel count. Use 4000 bytes for encoded scratch;
a smaller output buffer can restrict the encoder's packet budget or fail.

An empty packet requests 120 ms of packet-loss concealment. The same destination
capacity rule applies.

## Encoder settings

The default application is `audio`. Stereo bitrate starts at 128000 bits/s and
mono at 96000 bits/s. Other defaults are complexity 10, unconstrained VBR, music
signal, 16-bit input depth, and DTX/FEC disabled.

```js
codec.setBitrate(128000)
codec.setComplexity(6)
codec.setVBR(true)
codec.setConstrainedVBR(false)
codec.setSignal('music')
codec.setDTX(false)
codec.setFEC(false)
codec.setPLP(0)
```

`setPLP` takes an expected packet-loss percentage from 0 to 100. Complexity is
clamped to 0 through 10; lowering it trades encoding work for quality decisions.
FEC depends on the encoder's mode, bitrate and loss settings.

For low-level controls, `applyEncoderCTL(request, value)` and
`applyDecoderCTL(request, value)` accept the supported integer setters from
libopus. Reset uses request 4028 and ignores the value. Getter requests and blob
controls require pointer arguments and are rejected by this interface.
`applyEncoderCtl` and `encoderCTL` remain aliases of `applyEncoderCTL`.

`OpusEncoder.create(rate, channels, application)` is equivalent to the constructor.
The read-only `sampleRate`, `channels` and `application` properties expose its
construction settings. `getVersion()` returns the linked libopus version, as do
`getOpusVersion()` and `opus_get_version_string()`.

## Probe media

```js
import { readFile } from 'node:fs/promises'
import { probe, probeSync } from '@toddynnn/voice-opus'

const input = await readFile('track.flac')
const info = await probe(input)
console.log(info.sampleRate, info.channels, info.duration)

const sameInfo = probeSync(input)
```

`probe` copies the input before handing it to a worker, so changing the original
buffer after the call does not affect that operation. `probeSync` borrows it for
the duration of the call and blocks the calling thread.

Both methods return `sampleRate`, `channels`, `framesPerBlock`, `codec`, `nFrames`,
`duration` and `metadata`. Duration is in whole seconds. The `CodecType` enum and
TypeScript declarations describe the result fields.

## Build targets

| Platform | Architectures |
|---|---|
| macOS | x64, arm64 |
| Windows MSVC | x64, ia32, arm64 |
| Linux glibc | x64, arm64, armv7, riscv64 |
| Linux musl | x64, arm64 |
| Android | arm64, armv7 |
| FreeBSD | x64 |

## Build from source

Install Rust 1.88 or newer, CMake, Ninja, and a C compiler for your target. Android
builds also need the NDK and `ANDROID_NDK_HOME`. Ninja must be on PATH; the Cargo
configuration selects it to preserve CMake's optimized Release flags on MSVC.

```sh
npm ci
npm run build
npm test
```

The build generates `index.js`, `index.d.ts` and the platform `.node` file. Run
`npm run format` and `npm run lint` for Rust formatting and checks. `npm run bench`
measures the Into methods on synthetic 48 kHz stereo input; it excludes stream
copies, filters and transport.

If an existing Cargo build directory used another CMake generator, clear that
build cache or pass a fresh `--target-dir` to the build. On Windows, a synced
folder can reject N-API's output-directory rename. Build outside that folder with
`npm run build -- --output-dir <directory>`, then copy the generated files back.

Release instructions are in [docs/releasing.md](docs/releasing.md).

## License

The binding is MIT licensed. The native addon includes libopus, Symphonia and
other dependencies; their licenses and source locations are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
