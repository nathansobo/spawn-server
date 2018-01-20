const assert = require('assert')
const {startServer, SpawnClient} = require('..')

suite('spawn server', () => {
  test('basic spawning', async () => {
    const server = await startServer()
    const client = new SpawnClient(server.clientParams)
    const child = client.spawn(process.argv[0], ['-e', 'process.stdout.write("hey"); process.stderr.write("ho"); process.exit(42)'])

    await new Promise(resolve => setTimeout(resolve, 300))
    let stdout = collectOutput(child.stdout)
    let stderr = collectOutput(child.stderr)
    let exit = waitForExit(child)

    assert.deepEqual(await Promise.all(stdout, stderr, exit), [
      'hey',
      'ho',
      42
    ])

    server.stop()
  })
})

function collectOutput (stream) {
  let output = ''
  stream.on('data', data => output += data.toString('utf8'))
  return new Promise(resolve => {
    stream.on('close', resolve(output))
  })
}

function waitForExit (child) {
  return new Promise(resolve => {
    child.on('exit', (code) => resolve(code))
  })
}
