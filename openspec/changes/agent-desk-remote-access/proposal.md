# Change: Watch and steer a run from a phone, without opening a port

## Why

A long run happens while the person is away from the desk. Today that means it is
invisible until they come back: they cannot see a gate waiting for an answer, cannot
read what an agent decided, and cannot stop something going wrong.

Every existing way to fix this asks for something GitWyrm will not have. Opening a port
means asking a developer to expose their machine. A hosted service means an account, a
server to run, and someone else's copy of their code. Both contradict what this product
is: local-first, no account, nothing of the user's leaving their machine except what
they send.

## What Changes

- Add a headless Agent Desk service inside the existing app: the same session data and
  the same commands, reachable over a connection rather than only through the window.
- Both the desktop and the phone dial **outward** to a rendezvous that routes ciphertext
  it cannot read. No inbound connection, no opened port, no NAT assumption.
- Pair a phone by showing a code on the desktop, following the device-code shape the
  existing provider sign-in already uses. Identity is per host and per account, never
  per IP: thousands of machines behind one address must not see each other.
- Expose only the Agent Desk surface, not the git client: watch a run, read a
  transcript, answer a gate, stop a run.
- When the desktop is asleep, say so plainly, naming the machine and when it was last
  seen. No relay-side storage, no wake tricks, no pretending.

## Impact

- Adds a service layer over `src-tauri/src/commands/agent_desk*`, which today are
  in-process Tauri calls with no protocol, no authentication and no streaming.
- Adds a pairing surface to Settings and a session list, transcript and gate view to a
  phone client.
- Adds a rendezvous the project must run, whose only job is routing sealed bytes between
  two ends that already trust each other. It stores nothing.
- Does not touch how a run works: the phone answers the same gate the window does,
  through the same policy.
