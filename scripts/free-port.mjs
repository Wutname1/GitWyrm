// Frees the Vite dev port before launch (kills stale Vite processes).
//
// Vite binds localhost, which on Windows can resolve to either 127.0.0.1 (IPv4)
// or [::1] (IPv6) -- and it often listens on the IPv6 side. netstat lists those
// as separate rows, so the scan has to catch both. It also has to match the
// port exactly: a bare `findstr :5173` also matches :51730, :51731, and any
// ephemeral client port that happens to contain the digits, so we anchor on the
// LISTENING state and parse the local-address column precisely.
//
// Timing is the other half of the job. `npm run test` runs this, then a full
// typecheck and Rust test suite -- twenty seconds or more -- and only then
// starts Vite. Freeing the port up front and never looking again left a wide
// window for a stale dev server to take it, which is exactly how `npm run test`
// kept dying on "Port 5173 is already in use" after reporting the port free.
// So this now waits until the port is actually free *and* is invoked again
// immediately before Vite binds (see the `dev` script in package.json).
import { execSync } from 'node:child_process'

// Kept in step with vite.config.ts. Overridable so the pre-dev check and any
// future second dev port can share this script.
const PORT = Number(process.argv[2] ?? process.env.GITWYRM_DEV_PORT ?? 5173)

/**
 * Return the set of PIDs listening on PORT (any address family).
 *
 * `netstat -p tcp` reports IPv4 only -- an IPv6-only listener does not appear
 * in it at all. Vite binds localhost, which on Windows usually resolves to
 * [::1], so the socket that actually blocks a dev launch is precisely the one
 * that filter hides. This cost us a "port is free" message followed seconds
 * later by Vite dying on EADDRINUSE. Ask for both families explicitly.
 */
function listeningPids() {
  const pids = new Set()

  for (const family of ['tcp', 'tcpv6']) {
    let out = ''
    try {
      out = execSync(`netstat -ano -p ${family}`, { encoding: 'utf8' })
    } catch {
      // tcpv6 is absent on a machine with IPv6 disabled; that is not an error.
      continue
    }

    for (const line of out.split('\n')) {
      const cols = line.trim().split(/\s+/)
      // Rows look like: TCP  <local>  <foreign>  LISTENING  <pid>
      if (cols[0] !== 'TCP' || cols[3] !== 'LISTENING') continue
      const local = cols[1] // "127.0.0.1:5173", "[::1]:5173", "[::]:5173"
      // The port is whatever follows the final colon -- works for both families.
      const port = local.slice(local.lastIndexOf(':') + 1)
      if (port !== String(PORT)) continue
      const pid = cols[4]
      if (pid && pid !== '0') pids.add(pid)
    }
  }

  return pids
}

function kill(pid) {
  console.log(`[free-port] killing PID ${pid} holding port ${PORT}`)
  try {
    execSync(`taskkill /PID ${pid} /F /T`, { stdio: 'ignore' })
    return true
  } catch {
    // Already gone between scan and kill, or not ours to kill. Either way the
    // recheck below decides whether the port actually came free.
    return false
  }
}

/** Block the current thread. Atomics.wait needs no child process, unlike the
 *  old `powershell Start-Sleep`, which cost ~100ms a turn and assumed PATH. */
function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms)
}

// Kill whatever is holding the port, then confirm it is really released:
// taskkill returns before Windows tears the socket down, so a naive "kill and
// go" hands Vite a port that is still in TIME_WAIT.
const deadline = Date.now() + 5000
let pids = listeningPids()

while (pids.size > 0 && Date.now() < deadline) {
  for (const pid of pids) kill(pid)
  sleep(150)
  pids = listeningPids()
}

if (pids.size > 0) {
  // Loud and non-zero: continuing means Vite fails a few seconds from now with
  // a much less obvious message, which is the failure this script exists to
  // prevent. Better to stop here and say which process to deal with.
  console.error(
    `[free-port] ERROR: port ${PORT} is still held by PID(s) ${[...pids].join(', ')}.`,
  )
  console.error('[free-port] Close that process (or reboot) and try again.')
  process.exit(1)
}

console.log(`[free-port] port ${PORT} is free`)
