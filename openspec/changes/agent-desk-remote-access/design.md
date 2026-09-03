# Design: remote access

## The constraints that decide the shape

Four, all from the product rather than from taste:

1. **No opened ports.** A developer will not expose their machine to use a git client.
2. **No hosted infrastructure holding user data.** There is no account, and the project
   does not want a server it must secure someone's code inside.
3. **Thousands of hosts behind one public address.** A large corporate VPN means an IP
   identifies nothing. Any design that keys on address is wrong before it starts.
4. **A sleeping desktop is offline, and says so.** No wake-on-LAN, no queue that
   pretends.

Together these rule out: port forwarding, a public endpoint on the desktop, a hosted
copy of session data, and IP-based identity. What is left is a rendezvous that both ends
dial outward to, carrying bytes it cannot read.

## The shape

The desktop holds an outbound connection to a rendezvous. The phone dials the same
rendezvous and asks for one specific desktop by its paired identity. The rendezvous
matches the two and copies bytes between them. It never holds a key that opens those
bytes, and it stores no content: if it is compromised entirely, what leaks is that two
identities talked and roughly when.

This is the same handoff shape the HearthShelf web app uses to reach direct
infrastructure, and the reason to reuse it is that its trust boundary is already
understood: the middle is a switchboard, not a service.

## Identity, and why not IP

A pairing is a keypair per desktop and per account, exchanged during pairing and stored
on both ends. The rendezvous routes on that identity. Address is never consulted, which
is what makes constraint 3 hold: ten thousand desktops behind one VPN address are ten
thousand distinct identities, and a phone can only ask for the one it paired with.

Pairing itself follows the device-code shape already in `ai/copilot.rs`: the desktop
shows a short code, the phone enters it, both sides prove they saw the same code. That
precedent matters because it is a flow this codebase already gets right.

## What is exposed

Roughly forty of the app's three hundred commands: the Agent Desk surface. Listing
sessions, reading a transcript, answering a gate, stopping a run.

The git client is deliberately not exposed. Not because it is harder, but because
"watch and steer a run" is a small, checkable surface and "drive my repository from my
phone" is not. A commit or a push from a phone is a decision this package does not want
to have made by accident.

## The gate is the point

The single most valuable remote action is answering an approval. That is what turns
"the run stopped and waited four hours" into "the run waited ninety seconds".

A remote approval must be the same approval: same policy check, same record, same
refusal semantics. The only difference is that the transcript records which device
answered, because a person coming back to their desk should be able to see that they
allowed something from their phone.

## When the desktop is asleep

Say so. Name the machine, say when it was last seen, refuse the action.

The temptation is a queue: accept the approval and deliver it when the machine wakes.
That is wrong here. An approval is a judgement about a specific moment, and a run that
has been stopped for six hours may not be a run the person still wants to allow. A
queued yes is a yes given to a question nobody re-asked.

## What has to be built that does not exist

Everything about this is new to the codebase. There is no server, no protocol, no
authentication, no streaming, and no session concept outside the process. Every command
today is an in-process call over local files. That is the honest scope: this is the
largest package in the Agent Desk plan, and it should be sequenced last.

## Open questions for implementation

- Whether the headless service is the same binary in a background mode, or a separate
  process. A background mode keeps one copy of the session logic; a separate process
  survives the window closing, which is exactly when remote matters most.
- What the rendezvous costs to run, and whether it can be stateless enough to be cheap.
  The project has no hosted infrastructure today and does not want a bill that scales
  with users.
- Whether a phone should see repository content at all (a diff, a file) or only the
  conversation. Showing a diff is genuinely useful and is also the point where "watch a
  run" becomes "read my source on someone else's network".
