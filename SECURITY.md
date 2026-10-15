# Reporting a security issue

The repository has no private advisory channel (GitHub's private vulnerability
reporting is not enabled). Open a [new issue](https://github.com/seba2390/iw4L/issues/new)
titled "Security contact request" that says only that you have a report and
which area it touches (network, update path, file access). Leave out every
detail of the problem; the maintainer replies with a private contact, and the
report goes there.

This is a channel, not a program: there is no bounty, no triage SLA and no
promised fix date. A single maintainer reads the reports.

## What is in scope

IW4L is an experimental runtime and its network is meant for arranged playtests
between people who agreed to play. Reports are most useful where a problem
reaches past that:

* code execution or file writes outside `iw4l-artifacts/` from map data, a
  replay, a demo file or another client's traffic
* anything that writes to, patches or deletes files in an original game
  installation — IW4L reads those, and only reads them
* the update path: a way for something other than the release host to deliver a
  binary that the launcher accepts
* leaking the machine's files, credentials or private keys to a peer or to the
  master

## What is already known and not a finding

* The protocol changes between builds and is unstable by design; two
  different releases failing to talk to each other is expected.
* The master relays between clients and vouches for nobody. Connecting to a
  match means trusting who you are playing with ([`docs/MASTER.md`](docs/MASTER.md)).
* IW4L parses game data that a local user chose to supply. Malformed data from
  your own install crashing the runtime is a bug, not a vulnerability.

## What a report needs

The commit or release, the OS, what data reproduced it, and the steps. A crash
backtrace or the offending file is more useful than a description of it. Do not
send `.env`, private keys, or copies of game files.
