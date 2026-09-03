# Tasks: remote access

## 1. Headless service

- [ ] 1.1 Extract the Agent Desk command surface behind a transport-neutral boundary, so
      the same call can arrive from the window or from a connection.
- [ ] 1.2 Decide and record whether the service runs in the app process or its own, with
      the reason (the window closing is exactly when remote matters).
- [ ] 1.3 Stream session events over that boundary, so a phone sees a run advance rather
      than polling.
- [ ] 1.4 Refuse every command outside the Agent Desk surface, by allowlist rather than
      by blocklist.
- [ ] 1.5 Unit tests: an allowlisted command answers; a git command is refused; the
      refusal names why.

## 2. Rendezvous

- [ ] 2.1 Both ends dial outward; nothing listens on the desktop. Prove it with a test
      that asserts no listening socket is opened.
- [ ] 2.2 Route by paired identity, never by address.
- [ ] 2.3 Carry sealed bytes only: the rendezvous holds no key that opens them and stores
      no content at rest.
- [ ] 2.4 Survive a dropped connection by re-dialling, without losing a pairing.
- [ ] 2.5 Tests: two desktops sharing one address never see each other's sessions; a
      relay with full visibility learns no transcript text.

## 3. Pairing

- [ ] 3.1 Pair with a code shown on the desktop, following the device-code shape in
      `ai/copilot.rs`.
- [ ] 3.2 Store the pairing on both ends; list paired devices in Settings with when each
      was last seen.
- [ ] 3.3 Revoke a pairing, effective immediately and without a restart.
- [ ] 3.4 Tests: an unpaired device reaches nothing; a revoked device is refused at once.

## 4. Phone client

- [ ] 4.1 List sessions, read a transcript, and see which are waiting on an answer.
- [ ] 4.2 Answer an approval gate, through the same policy as the window.
- [ ] 4.3 Stop a run.
- [ ] 4.4 Record which device answered a gate, and show it in the transcript.
- [ ] 4.5 Tests: a remote approval is recorded identically to a local one, plus its
      device.

## 5. Unreachable desktop

- [ ] 5.1 Say plainly that the machine is not reachable, naming it and when it was last
      seen.
- [ ] 5.2 Refuse an approval rather than queueing it for delivery later.
- [ ] 5.3 Tests: an unreachable desktop produces a refusal, never a stored answer.

## 6. Acceptance

- [ ] 6.1 Native: pair a phone from a different network and watch a real run.
- [ ] 6.2 Native: answer a gate from the phone and confirm the run continues and the
      transcript records the device.
- [ ] 6.3 Native: confirm with a port scan that the desktop opened nothing.
- [ ] 6.4 Native: two desktops behind one public address, confirmed not to cross-talk.
- [ ] 6.5 Native: put the desktop to sleep and confirm the phone says so plainly.
- [ ] 6.6 Record Gate evidence for this package.
