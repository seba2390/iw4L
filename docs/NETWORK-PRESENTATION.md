# Shared Network Presentation

These shared client policies apply to live games, not only BO2 Zombies.
They do not change snapshot frequency, the 25 kB/s byte budget, stale-ACK
backoff, input/prediction, or the remote-player buffer policy.

## Remote Clock

After a previously steady arrival-offset window shifts ahead, catch-up can
accelerate at up to 20% above elapsed wall time. At least five recent offsets
must agree within a 400 ms window, with spread at most one frame or 8 ms.
Noisy arrivals retain conservative packet-based corrections. Remote time does
not rewind; a large positive discontinuity resets forward and clears recovery
history. A large negative offset does not take the listen-clock reset path.
Resetting the clock clears fractional correction and arrival history.

## Entity Poses

Live script-mover/actor root poses and animation-tree interpolation use the
same fixed 50 ms view delay rather than the variable received-snapshot interval.
Late root updates can use at most 50 ms of the latest authoritative trajectory,
then hold. Fresh/reset entities do not extrapolate without prior pose history.
Received stops and teleport resets take priority. Archived replay sampling is
unchanged.

Bounded trajectory prediction can be wrong after an unseen stop or turn, and
correct when its packet arrives. It does not predict native AI or guarantee
continuous motion through a disconnection.

## Validation Limits

Local synthetic position measurements informed these policies; they do not
certify rendered animation, camera correction, prediction, hit registration,
actual sockets/QUIC behavior, or impaired live multiplayer quality.

Some measured noisy-link cases regressed: longest entity hold increased from
284 to 334 ms in one mixed-loss case, and another sustained recovery increased
from 4584 to 4734 ms. Those remain open, not accepted no-regression results.

Before merge, compare the unchanged baseline and this proposal in real
two-client MW2 and BO1 multiplayer sessions, on both clean and impaired links.
Record clock resets/lag, entity motion/holds/corrections, native animation,
player presentation, packet pacing and disconnect/rejoin behavior. No such
live no-regression evidence is claimed here. Source checks cannot replace it.
