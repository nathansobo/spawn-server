const cp = require('child_process')
const path = require('path')
const net = require('net')

const serverPath = path.join(__dirname, 'server', 'target', 'release', 'spawn-server')

class SpawnServer {
  start (port) {
    this._child = cp.spawn(serverPath, {
      stdio: ['ignore', 'pipe', 'inherit']
    })

    let stdout = ''
    return new Promise(resolve => {
      this._child.stdout.on('data', data => {
        stdout += data
        if (stdout.endsWith('\n')) {
          this.clientParams = JSON.parse(stdout)
          resolve()
        }
      })
    })
  }

  stop () {
    this._child.kill()
  }
}

exports.SpawnClient =
class SpawnClient {
  constructor ({port, token}) {
    this._nextId = 0
    this._port = port
    this._token = token
    this._childrenById = new Map()
    this._pendingData = null
  }

  async spawn (path, args) {
    const socket = await this._getSocket()
    const id = this._nextId++
    const message = JSON.stringify({
      token: this._token,
      id,
      path,
      args,
      cwd: process.cwd(),
      env: process.env
    })
    socket.write(message)

    this._childrenById.set(id, new ChildProcess())
  }

  _getSocket () {
    if (!this._socketPromise) {
      const socket = net.createConnection(this._port)
      socket.on('data', data => this._receiveData(data))
      this._socketPromise = new Promise(resolve => {
        socket.on('connect', () => resolve(socket))
      })
    }

    return this._socketPromise
  }

  _receiveData (data) {
    if (this._pendingData) {
      data = this._pendingData.concat(data)
    }

    let offset = 0
    const requestId = data.readUint32BE(offset)
    offset += 4
    const type = data.readUint8(offset)
    offset += 1

    const child = this._childrenById.get(id)

    switch (type) {
      case MESSAGE_TYPE_EXIT:
        break
      case MESSAGE_TYPE_STDOUT:
        break
      case MESSAGE_TYPE_STDERR:
        break
    }
  }
}

class ChildProcess extends EventEmitter {
  constructor () {
    this.stdout = new Readable()
    this.stderr = new Readable()
  }
}

exports.startServer = async function startServer () {
  const server = new SpawnServer()
  await server.start()
  return server
}
