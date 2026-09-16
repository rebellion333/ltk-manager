# Champion select

## Changes

| Date       | Change                                                         |
| ---------- | -------------------------------------------------------------- |
| 2026-09-16 | Favourites per champion, beside the preferences and not inside |
| 2026-09-16 | A preference for an uninstalled mod reads as no preference     |
| 2026-09-16 | First draft: the panel, the scheduler, and the measured budget |

Each edit of this document adds a row at the top. The table keeps the last ten rows.

Champion select is the LTK Manager feature for the ninety seconds before a game starts. The
core design idea is that the pick is the trigger. The manager watches the League client,
sees which champion the player is taking, offers the mods installed for that champion, and
rebuilds the overlay for the one they choose while the select is still running.

What it replaces is a habit: knowing your champion before queueing, alt-tabbing to the
manager, enabling a mod, rebuilding, and hoping the build finished. The manager already
does every one of those steps. This feature does them at the moment the player actually
knows which champion they are playing, which is after the queue rather than before it.

## Goals

- A player who hovers a champion sees that champion's mods without navigating anywhere
- One click applies a mod, and the overlay carries it in the game that is about to start
- A swap that cannot make the deadline is refused and says so, rather than half-applying
- The choice is remembered, so the next game with that champion needs no clicks at all
- Nothing the manager does here touches the account: the client is read, never driven

## Feature status

- **Available** - the feature is in the application today
- **In progress** - work started, and the feature is not complete
- **Planned** - the team agreed on the feature, and work did not start
- **Proposed** - an idea for review, and not a decision

| Feature                 | Status    | Note                                                        |
| ----------------------- | --------- | ----------------------------------------------------------- |
| Client watch            | Available | Lockfile, WebSocket, and the gameflow phase                 |
| Champion select reducer | Available | Ten players reduced to the local cell before it crosses IPC |
| Game process timing     | Available | Polled beside the client's events, on one clock             |
| Champion roster         | Available | From the client, cached, refreshed whenever one answers     |
| Per-champion preference | Available | On the active profile, beside its enabled set               |
| Swap decision           | Available | The budget against the time left, with a named refusal      |
| Whole-archive rebuild   | Available | The swap path never takes the in-place tail write           |
| Scheduler               | Available | Coalesces the picks, applies one at a time                  |
| The panel               | Available | Raises itself on the first hover, one click to apply        |
| Favourites per champion | Available | A star per row, and the order the panel offers them in      |
| The bench               | Proposed  | ARAM's bench champions are carried and not drawn            |
| Precomputation          | Proposed  | Build the likely archive before the pick, not after         |

## Scope

The feature is one decision repeated: which champion is the player going to play, and does
a rebuild for it still fit before the game reads the archive.

Out of scope, and each of these is a constraint rather than a backlog item:

- **Anything the account does.** The client is read and never driven. The manager does not
  create a lobby, accept a queue, pick, ban, trade, or send a message. A player who wants
  the game launched still presses Play, which asks the Riot Client to start the game and
  says nothing to the account
- **Anti-cheat.** The manager hides nothing, hooks nothing it does not already hook, and
  spoofs nothing. If a design ever needs Vanguard not to see something, the design is wrong
- **Riot's own skins.** The manager handles the mods a player installed. It does not offer,
  fetch, or apply official cosmetics
- **Teammates.** The session the client publishes names ten players. The backend reduces it
  to the local player's cell before anything crosses IPC, so no screen can leak the rest
- **Idle cost.** The app runs while the player games. A measurable idle cost over the
  manager's own baseline is a bug in this feature, not a tradeoff it is allowed to make

## Vocabulary

| Word       | Meaning                                                                      |
| ---------- | ---------------------------------------------------------------------------- |
| Pick       | The champion the local player is taking: the lock, or the hover before it    |
| Swap       | Applying one champion's mod and rebuilding the overlay for it                |
| Tail       | The run from champion select ending to the game opening the champion's WAD   |
| Budget     | What the running machine has learned about its own rebuild cost and its tail |
| Refusal    | A named reason a swap was not attempted, with the numbers behind it          |
| Preference | The mod a champion applies, recorded on the active profile                   |

## The budget

The whole feature rests on one question with a measured answer: after the player can no
longer change champion, how long is there before the game reads that champion's archive.

| Leg                                              | Measured                   |
| ------------------------------------------------ | -------------------------- |
| Last possible champion change to `GAME_STARTING` | The rest of `FINALIZATION` |
| `GAME_STARTING` to the game opening the WAD      | 3.689 s and 4.098 s        |
| Rebuilding one champion's whole archive          | 424 ms to 553 ms (p95 553) |

`FINALIZATION` is 10 s in Practice Tool and blind pick, 30 s in draft, and 45 s in ARAM -
but ARAM's usable budget is the shortest of the four, at 9.9 s, because the bench lets the
champion change until the phase ends. The design number is therefore ten seconds, not
thirty, and draft is the generous mode rather than the reference one.

Against ten seconds plus a 3.7 s tail, a 553 ms rebuild is about a seventh of what is
available. `Budget::fits` still halves the margin before comparing, because a rebuild that
fits with nothing to spare misses whenever the machine has a bad moment, and the cost of
being wrong is a game played with the wrong mod.

**Every number above came off one PC with an NVMe drive**, which is why none of them is
compiled in as a promise. They are the value a fresh install starts from. The running
machine replaces its own rebuild estimate with what it measured the first time it rebuilds,
and a machine that cannot make the time refuses instead of trying. See
`crates/ltk-manager-core/src/champ_select/budget.rs`, whose doc comments carry the
provenance beside each constant.

The premise underneath all of it was verified rather than assumed: an archive written into
a live overlay **after** the DLL attached is still served. A champion's archive copied in 50
seconds after injection was redirected 1.277 s from attach, against 1.265 s for one that had
been there the whole time. The overlay is resolved when the game opens the file, not indexed
when the DLL goes in.

## The surfaces

| Surface            | Where                                | Says                                               |
| ------------------ | ------------------------------------ | -------------------------------------------------- |
| `ChampSelectPanel` | Global, on the first hover of a pick | The champion, its mods, and what the last swap did |
| The library        | `/mods`, per mod                     | Unchanged. A preference moves the same enabled set |

### The panel

It raises itself, which puts it in the `DIALOG_ORDER` queue of ADR-0022, and it goes first
in that order. Every other dialog in the queue is still true a minute later; this one is the
only one with a deadline.

It waits for a champion rather than for the select. A champion select that has opened with
nothing hovered has nothing to offer, and a dialog that opens on an empty list is one the
reader dismisses before it can say anything. The first hover is the moment it has something
to show, and hovering is enough - no lock needed, because the pick can still move after a
lock and the scheduler follows it either way.

It also waits for the champion roster, which is what turns the numeric id champion select
speaks in into the alias the library joins mods on. A roster that has not answered yet is
indistinguishable from one that cannot name the champion, and raising before it answers
would flash "unrecognised champion" across every game.

The body is the champion's mods, one row each, plus a row for applying none. Clicking a row
is the whole interaction. Under the rows is one line saying what actually happened, and
under that, what a swap costs on this machine and how long there is.

Closing the panel is about that champion select. It does not turn the feature off, it does
not stop the scheduler, and the next champion select raises it again.

**It takes the screen.** The panel opens in a window the reader is not looking at, because
they are in the League client. A taskbar flash was tried first and went unnoticed, so the
window brings itself forward instead, from the backend rather than from the panel: a
minimized webview is not a reliable place to run anything, and a frontend attempt left no
way to tell whether it had run at all. This interrupts banning and trading,
which is the cost of arriving while the choice can still be acted on. It happens only when
there is a choice to make: mods installed for that champion and nothing recorded for it.
A champion whose mod is already decided is swapped without the reader ever being told.

### The status line

The line is the honest half of the panel. A swap that did not happen says so, names why, and
says what the reader keeps instead.

| Refusal              | Says                                                           |
| -------------------- | -------------------------------------------------------------- |
| `patcherIdle`        | There is no overlay to swap. The choice is saved for the build |
| `patcherBuilding`    | The choice goes into the build already running                 |
| `gameAlreadyRunning` | Too late for this game. The choice applies to the next one     |
| `gameStarting`       | The same, from the deadline rather than from the process       |
| `champSelectOver`    | The same, from the select having ended                         |
| `buildInFlight`      | A build of ours is running, so the swap waits for it           |
| `notEnoughTime`      | What a rebuild needs and what was left, both in milliseconds   |

Each crosses IPC as a code with its typed fields, per ADR-0017, and the sentence above is
the frontend's. `notEnoughTime` is the one that carries numbers, and it carries both of
them: a reader who is told a swap did not fit deserves to see the two figures that did not
fit.

Only four of the seven ever reach the line. The scheduler reports a refusal it can never
take back and stays quiet about one a later tick could turn into a swap, because repeating
"still building" once a second says the same thing sixty times. That is right for the two
that resolve on their own, and wrong for `patcherIdle`, which lasts the whole select and
would otherwise answer a click with nothing at all. So the panel draws that one standing,
from the patcher's own status, before anything is clicked.

## What a choice does

One call moves two things together, which is why it is one call. The profile records the
mod the champion applies, and the profile's enabled set stops carrying any other mod for
that champion. Recording without applying leaves the preference a lie until the next build;
applying without recording loses the choice the moment anything else rewrites the profile.

Mods for other champions are left alone. A profile is a whole set, and a swap is about one
champion of it.

The rebuild is not part of that call. The scheduler notices that what the overlay should be
carrying has changed, decides whether it fits, and reports - so a choice made by hand in the
panel and a choice applied automatically from a preference take the same path and cannot
drift. A choice made with no game in sight is a preference like any other, and the next
build picks it up.

### Favourites are not choices

A star keeps a mod at the top of that champion's list, in the order the reader marked them.
It moves nothing else: no mod is enabled or disabled, no overlay is rebuilt, and the
scheduler is not told. The write is optimistic for exactly that reason - being wrong for one
frame costs a star drawn filled that empties again, where being wrong about what the
champion applies costs a game played with the wrong mod.

They live in their own map on the profile rather than inside `ChampionPreference`, and the
reason is worth stating because the shape invited the opposite. **An entry in the
preferences is a decision**, and champion select acts on one every game; an entry with no
mod in it is the reader having chosen _no_ mod, which switches off what they have. Held
together, marking a favourite would write a preference, so ordering a list would turn mods
off. Apart, neither can be mistaken for the other.

**A choice outlives the mod it named, and that is not a failure.** Uninstalling leaves the
preference pointing at an id the library no longer holds, and a reinstall issues a new one,
so the old id never comes back. Such a preference is read as no preference at all: the
champion is left alone, the panel shows nothing chosen, and choosing again overwrites it.
The whole entry is what goes rather than the mod alone, because an entry with no mod is the
reader having chosen _no mod_, which switches off the ones they have.

## Why a swap rebuilds whole archives

The overlay builder has two write paths. One writes a temporary file and renames it over the
old one; the other sets the length of the existing file and overwrites it in place. The
second is faster and is what the builder takes when its recorded layout says it can.

A swap always takes the first. A game that is loading may already have the archive open, and
an in-place overwrite of a file another process is reading is not a decision the manager is
entitled to make on a player's behalf. A rename leaves the path pointing at new bytes and
never modifies what an open handle is looking at. The cost of that choice is the difference
between 553 ms and something smaller, against a budget of ten seconds. See ADR-0043.

## Open questions

- **A replaced archive.** The live test added an archive to an overlay and watched it serve.
  Replacing one is the same operation from `CreateFile`'s point of view, and that is an
  argument rather than a measurement
- **Process to DLL.** The one leg of the timeline with no number, because the client watch
  and the patcher log were never running at the same time
- **Slow disks.** Every figure here is from an NVMe. The mechanism adapts, and nobody has
  yet watched it adapt on a mechanical drive
