// Frees port 5173 before dev launch (kills stale Vite processes).
import { execSync } from 'node:child_process'

const PORT = 5173

try {
  const out = execSync(`netstat -ano -p tcp | findstr :${PORT} | findstr LISTENING`, {
    encoding: 'utf8',
  })
  const pids = new Set(
    out
      .trim()
      .split('\n')
      .map((line) => line.trim().split(/\s+/).pop())
      .filter((pid) => pid && pid !== '0')
  )
  for (const pid of pids) {
    console.log(`[free-port] killing PID ${pid} holding port ${PORT}`)
    execSync(`taskkill /PID ${pid} /F`, { stdio: 'ignore' })
  }
} catch {
  // findstr exits non-zero when nothing matches: port is already free.
}
console.log(`[free-port] port ${PORT} is free`)
