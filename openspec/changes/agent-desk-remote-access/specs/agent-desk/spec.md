# Agent Desk Spec Delta

## ADDED Requirements

### Requirement: Nothing listens for an inbound connection

The desktop SHALL NOT open a listening port, request a firewall exception, or depend on
port forwarding, UPnP or any inbound reachability. Both ends SHALL establish the
connection by dialling outward.

#### Scenario: No port is opened

- WHEN remote access is enabled and a phone is paired
- THEN the desktop has opened no listening port and asked for no firewall change

#### Scenario: Behind a restrictive network

- WHEN the desktop is on a network that blocks all inbound traffic
- THEN remote access still works

### Requirement: The rendezvous cannot read what it carries

Session content, transcripts, approvals and repository data SHALL be readable only by
the paired desktop and phone. The rendezvous SHALL route sealed bytes, SHALL NOT hold a
key that can open them, and SHALL NOT store message content at rest.

#### Scenario: The relay is compromised

- WHEN someone with full access to the rendezvous reads everything it handles
- THEN they learn no transcript text, no repository content and no approval detail

### Requirement: Identity is per host and per account

A pairing SHALL identify one desktop and one account, never a network address. Two
desktops sharing a public address SHALL NOT be able to reach, list or receive each
other's sessions.

#### Scenario: Many machines behind one address

- WHEN thousands of desktops connect from the same public address
- THEN each phone reaches only the desktop it paired with

### Requirement: Pairing is explicit and revocable

Pairing SHALL require an action on the desktop, SHALL show what is being paired, and
SHALL be listed and revocable afterwards. Revoking SHALL take effect without restarting
the app.

#### Scenario: A lost phone

- WHEN a paired phone is revoked
- THEN it can no longer read sessions or answer gates, immediately

### Requirement: Remote reaches Agent Desk only

The remote surface SHALL expose watching a run, reading a transcript, answering an
approval gate, and stopping a run. It SHALL NOT expose the git client: no commit, no
push, no branch operation, no arbitrary file read.

#### Scenario: Git is not remote

- WHEN a paired phone is connected
- THEN no git operation can be started from it

### Requirement: An approval from a phone is the same approval

A gate answered remotely SHALL pass through the same policy as one answered in the
window, SHALL be recorded identically, and SHALL show which device answered it.

#### Scenario: A remote answer is attributable

- WHEN a gate is allowed from a phone
- THEN the transcript records the approval and that it came from that device

### Requirement: A sleeping desktop is reported plainly

When the desktop is unreachable, the phone SHALL say so in plain words, name the machine,
and say when it was last seen. It SHALL NOT queue work for later delivery, SHALL NOT
imply the machine is reachable, and SHALL NOT attempt to wake it.

#### Scenario: The desktop is asleep

- WHEN a person opens the phone client while their desktop is asleep
- THEN they are told the machine is not reachable and when it was last seen

#### Scenario: Nothing is queued

- WHEN a person tries to answer a gate while the desktop is unreachable
- THEN the answer is refused rather than stored to be delivered later
