const assert = require('node:assert/strict')
const { performance } = require('node:perf_hooks')
const { OpusEncoder, getOpusVersion } = require('../index.js')

const iterations = Number(process.argv[2] || 5000)
assert.ok(
  Number.isSafeInteger(iterations) && iterations > 0,
  'Iterations must be a positive integer'
)
const frames = Array.from({ length: 100 }, (_, frame) => {
  const pcm = Buffer.alloc(3840)
  for (let sample = 0; sample < 960; sample++) {
    const position = frame * 960 + sample
    const left = Math.round(Math.sin(position * 0.057) * 12000)
    const right = Math.round(Math.sin(position * 0.071) * 10000)
    pcm.writeInt16LE(left, sample * 4)
    pcm.writeInt16LE(right, sample * 4 + 2)
  }
  return pcm
})
const packetEncoder = new OpusEncoder(48000, 2, 'audio')
const packets = frames.map((frame) => packetEncoder.encode(frame))
const results = []
for (const operation of ['encodeInto', 'decodeInto']) {
  const codec = new OpusEncoder(48000, 2, 'audio')
  const inputs = operation === 'encodeInto' ? frames : packets
  const output = Buffer.alloc(operation === 'encodeInto' ? 4000 : 23040)
  for (let index = 0; index < 500; index++)
    codec[operation](inputs[index % inputs.length], output)
  const rounds = []
  for (let round = 0; round < 5; round++) {
    const startCpu = process.cpuUsage()
    const start = performance.now()
    for (let index = 0; index < iterations; index++)
      codec[operation](inputs[index % inputs.length], output)
    const wallUs = ((performance.now() - start) * 1000) / iterations
    const cpu = process.cpuUsage(startCpu)
    rounds.push({ wallUs, cpuUs: (cpu.user + cpu.system) / iterations })
  }
  const median = (values) =>
    [...values].sort((first, second) => first - second)[2]
  results.push({
    operation,
    wallUs: median(rounds.map((round) => round.wallUs)),
    cpuUs: median(rounds.map((round) => round.cpuUs))
  })
}
console.log(
  `${getOpusVersion()}, ${process.version}, ${process.platform}/${process.arch}`
)
console.log(
  `48 kHz stereo, 20 ms, complexity 10; ${iterations} calls per round`
)
console.table(results)
console.log(
  'Synthetic input; these numbers exclude stream ownership copies, filters and transport'
)
