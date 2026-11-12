const assert = require('node:assert/strict')
const test = require('node:test')
const path = require('node:path')
const binding = require(
  process.env.CODEC_PATH ? path.resolve(process.env.CODEC_PATH) : '../index.js'
)
const { OpusEncoder } = binding

test('uses bundled libopus 1.6.1', () => {
  assert.equal(binding.getOpusVersion(), 'libopus 1.6.1')
})

function pcm(rate = 48000, channels = 2, duration = 20, offset = 0) {
  const buffer = Buffer.alloc(((rate * duration) / 1000) * channels * 2)
  for (let sample = 0; sample < buffer.length / 2; sample++) {
    buffer.writeInt16LE(
      Math.round(Math.sin((sample + offset) * 0.073) * 12000),
      sample * 2
    )
  }
  return buffer
}

test('undersized output does not advance the decoder', () => {
  const encoder = new OpusEncoder(48000, 2, 'audio')
  const packets = [
    encoder.encode(pcm()),
    encoder.encode(pcm(48000, 2, 20, 900))
  ]
  const attempted = new OpusEncoder(48000, 2, 'audio')
  const control = new OpusEncoder(48000, 2, 'audio')
  attempted.decode(packets[0])
  control.decode(packets[0])
  assert.throws(() => attempted.decodeInto(packets[1], Buffer.alloc(1)))
  assert.ok(
    attempted.decode(packets[1]).equals(control.decode(packets[1])),
    'failed output must not consume decoder state'
  )
})

if (!process.env.REGRESSION_ONLY) {
  test('allocating and Into APIs agree across rates, channels and frame durations', () => {
    for (const rate of [8000, 12000, 16000, 24000, 48000]) {
      for (const channels of [1, 2]) {
        for (const duration of [2.5, 5, 10, 20, 40, 60]) {
          const first = new OpusEncoder(rate, channels, 'audio')
          const second = OpusEncoder.create(rate, channels, 'audio')
          const input = pcm(rate, channels, duration)
          const destination = Buffer.alloc(4000, 0xaa)
          const packet = first.encode(input)
          const written = second.encodeInto(input, destination)
          assert.deepEqual(
            destination.subarray(0, written),
            packet,
            `${rate}/${channels}/${duration}`
          )
          assert.equal(destination[written], 0xaa)
          const expected = first.decode(packet)
          const output = Buffer.alloc(rate * channels * 0.12 * 2 + 3, 0xaa)
          const decoded = second.decodeInto(packet, output)
          assert.equal(decoded, input.length)
          assert.deepEqual(output.subarray(0, decoded), expected)
          assert.equal(output[decoded], 0xaa)
        }
      }
    }
  })

  test('unaligned Uint8Array input and output, disjoint views of the same buffer', () => {
    const input = pcm()
    const storage = new Uint8Array(input.length + 4002)
    storage.set(input, 1)
    const unaligned = storage.subarray(1, input.length + 1)
    const output = storage.subarray(input.length + 2)
    const first = new OpusEncoder(48000, 2)
    const second = new OpusEncoder(48000, 2)
    const written = first.encodeInto(unaligned, output)
    assert.deepEqual(
      Buffer.from(output.subarray(0, written)),
      second.encode(input)
    )
    const decoded = new Uint8Array(23041).subarray(1)
    assert.equal(first.decodeInto(output.subarray(0, written), decoded), 3840)
    assert.deepEqual(
      Buffer.from(decoded.subarray(0, 3840)),
      second.decode(output.subarray(0, written))
    )
  })

  test('validates buffers, packet geometry and integer CTL requests', () => {
    const codec = new OpusEncoder(48000, 2)
    const input = pcm()
    assert.throws(() => codec.encodeInto(input, input), /overlap/)
    assert.throws(() => codec.encode(Buffer.alloc(3)))
    assert.throws(() => codec.encode(Buffer.alloc(8)))
    assert.throws(() => codec.decodeInto(Buffer.from([3]), Buffer.alloc(23040)))
    assert.throws(() =>
      codec.decodeInto(Buffer.from([3, 0]), Buffer.alloc(23040))
    )
    assert.throws(() =>
      codec.decodeInto(Buffer.from([3, 63]), Buffer.alloc(23040))
    )
    codec.setDTX(true)
    codec.setConstrainedVBR(false)
    codec.setSignal('music')
    codec.applyDecoderCTL(4034, 0)
    assert.throws(() => codec.applyEncoderCTL(9999, 0))
    assert.throws(() => codec.applyEncoderCTL(4052, 0), /integer setter/)
    assert.throws(() => codec.applyEncoderCTL(4003, 0), /integer setter/)
    codec.setDTX(false)
    codec.setFEC(true)
    codec.setPLP(10)
    codec.setVBR(false)
    codec.setComplexity(6)
    codec.applyEncoderCTL(4002, 64000)
    assert.ok(codec.encode(input).length)
  })

  test('reset CTLs restore encoder and decoder history', () => {
    const codec = new OpusEncoder(48000, 2)
    const input = pcm()
    const packet = codec.encode(input)
    const expected = codec.decode(packet)
    codec.encode(pcm(48000, 2, 20, 900))
    codec.decode(packet)
    codec.applyEncoderCTL(4028, 0)
    codec.applyDecoderCTL(4028, 0)
    assert.deepEqual(codec.encode(input), packet)
    assert.deepEqual(codec.decode(packet), expected)
  })

  test('120 ms multi-frame packets and PLC retain output bounds', () => {
    const codec = new OpusEncoder(48000, 2)
    const encoder = new OpusEncoder(48000, 2)
    const packet = encoder.encode(pcm())
    // Code 3 CBR packet containing six identical 20 ms CELT frames.
    const combined = Buffer.concat([
      Buffer.from([(packet[0] & ~3) | 3, 6]),
      ...Array(6).fill(packet.subarray(1))
    ])
    const out = Buffer.alloc(23041, 0xaa)
    assert.equal(codec.decodeInto(combined, out), 23040)
    assert.equal(out[23040], 0xaa)
    assert.equal(codec.decodeInto(Buffer.alloc(0), out), 23040)
  })

  test('probe exports still read a PCM WAV', async () => {
    const module = binding
    const audio = pcm()
    const header = Buffer.alloc(44)
    header.write('RIFF')
    header.writeUInt32LE(36 + audio.length, 4)
    header.write('WAVEfmt ', 8)
    header.writeUInt32LE(16, 16)
    header.writeUInt16LE(1, 20)
    header.writeUInt16LE(2, 22)
    header.writeUInt32LE(48000, 24)
    header.writeUInt32LE(192000, 28)
    header.writeUInt16LE(4, 32)
    header.writeUInt16LE(16, 34)
    header.write('data', 36)
    header.writeUInt32LE(audio.length, 40)
    const wav = Buffer.concat([header, audio])
    const result = module.probeSync(wav)
    assert.equal(result.sampleRate, 48000)
    assert.equal(result.channels, 2)
    const { pbkdf2 } = require('node:crypto')
    const busy = Array.from(
      { length: 4 },
      () =>
        new Promise((resolve, reject) => {
          pbkdf2('thread-pool', 'snapshot', 100000, 16, 'sha256', (error) =>
            error ? reject(error) : resolve()
          )
        })
    )
    const pending = module.probe(wav)
    wav.fill(0)
    const results = await Promise.all([...busy, pending])
    assert.deepEqual(results.at(-1), result)
  })
}
