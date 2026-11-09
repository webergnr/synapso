# Security policy

## Reporting a vulnerability

**Please do not open a public issue for a security problem.** An issue is
visible to everyone the moment it exists, including to whoever would use it.

Report it privately through GitHub instead:

**Security → Advisories → Report a vulnerability**
<https://github.com/webergnr/synapso/security/advisories/new>

That opens a draft advisory only the maintainer can see, and it is the place to
discuss a fix and a disclosure date.

Expect a first reply within about a week. This is a personal project with one
maintainer, not a funded product — please read that as an honest estimate
rather than a service level.

## What is in scope

Synapso is a **local, single-user desktop app**. It reads and writes a folder
you own, and it does not run a server, open a port, or send your notes
anywhere.

That shapes what counts:

**In scope**

- Anything that reads or writes outside the vault folder you chose
- Anything that makes the app execute content from a `.md`, `.canvas` or image
  file rather than merely display it
- Anything that destroys or silently corrupts your files
- Crashes reachable from a file the app is expected to open — including
  malformed ones, which is the realistic case if you sync a vault or open one
  someone sent you

**Out of scope, today**

- The vault is trusted input. It is your folder; a `../` inside a file you own
  crosses no trust boundary that exists yet. This changes the day Synapso grows
  a server, an MCP surface, or any path that comes from someone else — see
  ADR-001, which records the containment work as deliberately deferred with
  that exact trigger.
- Anything requiring an attacker to already have your user account.
- Denial of service against yourself.

## Known and already written down

The repository tracks its own defects in the open, and some of them are
integrity problems rather than security ones. Before reporting, it is worth
checking the open issues and the punch lists in `docs/adr-*`.
Something already listed there is not a surprise — but if you have found a way
to reach one of them from a *file* rather than from the UI, that is new, and
worth reporting privately.

## Releases

Binaries are currently **unsigned**, and macOS builds are ARM64 only. There is
no automatic update channel. Fixes ship as a new tagged release; you replace
the app yourself.
