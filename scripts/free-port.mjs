// Frees port 5173 before dev launch (kills stale Vite processes).
//
// Vite binds localhost, which on Windows can resolve to either 127.0.0.1 (IPv4)
// or [::1] (IPv6) -- and it often listens on the IPv6 side. netstat lists those
// as separate rows, so the scan has to catch both. It also has to match the
// port exactly: a bare `findstr :5173` also matches :51730, :51731, and any
// ephemeral client port that happens to contain the digits, so we anchor on the
// LISTENING state and parse the local-address column precisely.
import { execSync } from 'node:child_process'

const PORT = 5173

/** Return the set of PIDs listening on PORT (any address family). */
function listeningPids() {
  let out = ''
  try {
    out = execSync('netstat -ano -p tcp', { encoding: 'utf8' })
  } catch {
    return new Set()
  }

  const pids = new Set()
  for (const line of out.split('\n')) {
    const cols = line.trim().split(/\s+/)
    // Rows look like: TCP  <local>  <foreign>  LISTENING  <pid>
    if (cols[0] !== 'TCP' || cols[3] !== 'LISTENING') continue
    const local = cols[1] // e.g. "127.0.0.1:5173" or "[::1]:5173" or "[::]:5173"
    // The port is whatever follows the final colon -- works for both families.
    const port = local.slice(local.lastIndexOf(':') + 1)
    if (port !== String(PORT)) continue
    const pid = cols[4]
    if (pid && pid !== '0') pids.add(pid)
  }
  return pids
}

function kill(pid) {
  console.log(`[free-port] killing PID ${pid} holding port ${PORT}`)
  try {
    execSync(`taskkill /PID ${pid} /F`, { stdio: 'ignore' })
  } catch {
    // Already gone between scan and kill -- fine.
  }
}

let pids = listeningPids()
for (const pid of pids) kill(pid)

// taskkill returns before the socket is fully released, so confirm the port is
// actually free before handing off to Vite. One short retry covers the gap.
if (pids.size > 0) {
  const deadline = Date.now() + 2000
  while (Date.now() < deadline) {
    pids = listeningPids()
    if (pids.size === 0) break
    for (const pid of pids) kill(pid)
    execSync('powershell -NoProfile -Command "Start-Sleep -Milliseconds 200"', {
      stdio: 'ignore',
    })
  }
}

if (pids.size > 0) {
  console.error(
    `[free-port] WARNING: port ${PORT} still held by PID(s) ${[...pids].join(', ')}`,
  )
} else {
  console.log(`[free-port] port ${PORT} is free`)
}
