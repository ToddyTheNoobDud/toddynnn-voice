const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const { parseTriple } = require('@napi-rs/cli')

const root = path.resolve(__dirname, '..')
const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'))
const writeJson = (file, value) =>
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`)
const manifestPath = path.join(root, 'package.json')
const manifest = readJson(manifestPath)
const checkOnly = process.argv.includes('--check')
assert.ok(
  manifest.napi.targets.length > 0,
  'Declare at least one native target'
)
const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8')
const cargoVersion = cargo.match(/^version = "([^"]+)"$/m)?.[1]
assert.equal(
  manifest.version,
  cargoVersion,
  'npm and Cargo versions must match'
)

const packageRoot = path.join(root, 'npm')
assert.ok(
  fs.existsSync(packageRoot),
  'Run npm run release:prepare after downloading all build artifacts'
)
const directories = fs
  .readdirSync(packageRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort()
assert.deepEqual(
  directories,
  manifest.napi.targets
    .map((target) => parseTriple(target).platformArchABI)
    .sort(),
  'Platform packages must match napi.targets'
)

const notices = ['LICENSE', 'THIRD_PARTY_NOTICES.md']
const optionalDependencies = {}
const platforms = []
// Validate every artifact before changing release metadata.
for (const directory of directories) {
  const location = path.join(packageRoot, directory)
  const file = path.join(location, 'package.json')
  const platform = readJson(file)
  const binary = `${manifest.napi.binaryName}.${directory}.node`
  assert.equal(
    platform.name,
    `${manifest.name}-${directory}`,
    `Unexpected package in ${directory}`
  )
  assert.equal(
    platform.version,
    manifest.version,
    `Stale version in ${directory}`
  )
  assert.equal(platform.main, binary, `Unexpected binary entry in ${directory}`)
  assert.ok(
    fs.existsSync(path.join(location, binary)),
    `Missing artifact: ${binary}`
  )
  assert.ok(
    fs.statSync(path.join(location, binary)).size > 0,
    `Empty artifact: ${binary}`
  )
  optionalDependencies[platform.name] = manifest.version
  platforms.push({ location, file, platform, binary })
}
for (const file of manifest.files) {
  assert.ok(
    fs.existsSync(path.join(root, file)),
    `Missing root package file: ${file}`
  )
}

for (const { location, file, platform, binary } of platforms) {
  const files = [binary, ...notices]
  for (const notice of notices) {
    const content = fs.readFileSync(path.join(root, notice), 'utf8')
    if (checkOnly) {
      assert.equal(
        fs.readFileSync(path.join(location, notice), 'utf8'),
        content,
        `Stale ${notice} in ${platform.name}`
      )
    } else {
      fs.writeFileSync(path.join(location, notice), content)
    }
  }
  if (checkOnly) {
    assert.deepEqual(
      platform.files,
      files,
      `Unexpected package contents in ${platform.name}`
    )
  } else {
    platform.files = files
    writeJson(file, platform)
  }
}
if (checkOnly) {
  assert.deepEqual(
    manifest.optionalDependencies,
    optionalDependencies,
    'Root optional dependencies must match the prepared platform packages'
  )
} else {
  manifest.optionalDependencies = optionalDependencies
  writeJson(manifestPath, manifest)
}
console.log(
  `${checkOnly ? 'Checked' : 'Prepared'} ${manifest.name}@${manifest.version} and ${platforms.length} platform packages`
)
