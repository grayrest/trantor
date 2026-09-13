# K1 — trantor-terminal: design log (2026-09-12)

**Plan:** [`plans/2026-09-12-k1-terminal.md`](../plans/2026-09-12-k1-terminal.md).

Settled against trantor `e3138d4`, trantor-cli `88bbcbc`. The brief: `Tty` left
trantor-cli in `98c97de` because its two raw-mode leaves had been empty bodies
behind a doc promising no echo and no Ctrl-C, and because termios is terminal
state rather than a CLI capability. It gets its own package, and that package
is designed rather than ported: raw mode is the one API in this ecosystem that
can outlive the process badly.

## What was found before any question was asked

- **Upstream's API is an unguarded pair.** basic-cli 0.21's `tty.roc` and
  `terminal-app-snake.roc` call `Tty.enable_raw_mode!()` / `disable_raw_mode!()`
  by hand. Both hold their `?` until after the disable, so the author knew; in
  `tty.roc` a failing `Stdout.line!(...)?` between the two still leaves the
  shell raw.
- **Upstream "raw" is full `cfmakeraw`, OPOST included.** The snake writes
  `\r\n` itself (`terminal-app-snake.roc:141–155`); under raw mode `\n` does not
  return the carriage.
- **The generated driver has no teardown hook.** `driver_lib`
  (`src/codegen.rs`) is `catch_unwind(roc_main)` then a gauge report;
  `roc_crashed` calls `process::exit(1)`.
- **Every in-process exit already passes through libc `exit()`:**

  | Exit path | through `exit()`? |
  |---|---|
  | `main!` returns `Ok`/`Err` | yes — driver `main` returns |
  | a component panics | yes — caught, `main` returns 70 |
  | `roc_crashed` | yes — `process::exit(1)` |
  | `Cli.exit!` | yes — `process::exit` (`cli-host/src/lib.rs:236`) |
  | SIGTERM / SIGHUP / SIGQUIT | no |
  | SIGTSTP | no — and the process stops still raw |
  | SIGKILL, `abort` | no, and uncatchable |

- **`is_terminal!` has a live consumer and no home.** rocjust's `--color auto`
  calls `Tty.is_terminal!(Stdout | Stderr)` (`rocjust/app/main.roc:319`), added
  in the local basic-cli fork. It was never in trantor-cli.
- **seahaven ships a `Signal` module** that installs SIGHUP/SIGINT/SIGQUIT
  handlers and records arrivals for `take!`. A terminal guard composed into the
  same process must not clobber it.
- **trantor has no Windows target.** Every world gets `arm64mac`/`x64mac`;
  `[world] targets` (D-H7-42) appends roc target names such as `arm64musl`.
- **signal-hook-registry 1.4.8 calls the previous handler *before* its own
  actions** (`src/lib.rs:464`: "If there was a previous signal handler for the
  given signal, it is chained ‒ it will be called as part of this library's
  signal handler, before any actions"). It chains only real handlers; a
  previous `SIG_DFL` is not invoked, so the default must be emulated.
- **None of the candidate crates compiles C.** `libc 0.2.189` and `rustix
  1.1.4` build scripts only probe `rustc`; `signal-hook-registry` has no build
  script; `signal-hook 0.4.4` calls `cc` only under the non-default
  `extended-siginfo-raw` feature. The Rust ecosystem's `atexit` is the `dtor`
  crate (0.8.1), whose wrapper is itself `extern "C" { fn atexit(..) }`
  (`src/lib.rs:133`, `__cxa_atexit` on Apple) — the same call as `libc::atexit`.
- **A pty discards queued output when the last slave fd closes**
  (`rocjust/DIVERGENCES.md`, the `--color auto` section). Any harness asserting
  on bytes a child wrote at exit must hold a slave fd of its own.

## Decisions

**D-K1-1 — the package covers six layers, batteries included, each higher
layer replaceable.** (a) terminal detection, (b) mode, (c) geometry and resize,
(d) timed input — hosted; (e) key decoding, (f) output — pure Roc.

The first proposal stopped at a–d on the grounds that e–f are pure libraries
that need neither the libc dependency nor the lifecycle risk. Rejected: a
terminal package that leaves every consumer to write a key decoder is not one
people use. The concern it answered — that e–f are big and opinionated — is met
by D-K1-10's component split instead of by omission.

**D-K1-2 — restoration is a host-side guard, not a composer teardown hook.**
On the first mode change the host saves the original termios and registers
`atexit(restore)`. That covers the first four rows of the exit table without a
trantor change. Handlers for SIGTERM/SIGHUP/SIGQUIT restore and then take the
default action, so the exit status still says "killed by signal". SIGTSTP
restores and stops; SIGCONT re-enters the mode, rewrites the enter bytes
(D-K1-9) and surfaces `Resumed` (D-K1-6). SIGKILL and `abort` are documented as
what `reset` is for.

A composer `on_exit` list for drivers was the alternative: cleaner in
principle, a trantor change in practice, and it sees no exit that `atexit` does
not. Signals would need handlers either way.

The Roc surface is scoped (`Terminal.with_mode!`, D-K1-8). The basic-cli `Tty`
shim keeps its unscoped pair for compatibility, and both sit on the same guard,
so neither can leak through a catchable exit.

**D-K1-3 — the guard chains to handlers already installed.** It never replaces
one. Settled here as "restore, then chain"; corrected at D-K1-11 to signal-hook's
actual order, "chain, then restore, then default".

**D-K1-4 — Unix only; the hosted surface is phrased in modes, not termios
flags.** macOS and Linux. Nothing in the Roc signatures names a termios bit, so
a Windows host (`SetConsoleMode`) could be added under the same surface when
trantor grows a Windows target. [ASSUMPTION stated during the grill, not
objected to.]

**D-K1-5 — the package talks to `/dev/tty`, opened once and kept.** Mode, size,
input and rendering all go through the controlling terminal regardless of how
stdin/stdout are redirected: `git log | picker > out.txt` works, with stdout
free for the program's result. That is fzf's and less's model.

`is_terminal!(Stdin | Stdout | Stderr)` still asks about the *standard streams*
— a different question, and the one `--color auto` needs. No controlling
terminal (cron, CI, `setsid`) is a named `NoTerminal` at open, not a crash.

Rejected: fd 0/1 (basic-cli, crossterm's default) — `ENOTTY` the moment stdin is
a pipe; and stdin-if-tty-else-`/dev/tty` — two code paths for one semantics.
The upstream examples still work under B: with stdin not redirected, fd 0 and
`/dev/tty` are the same device, so a mode set through one applies to the other.

**D-K1-6 — one blocking `read!` with a timeout returns whichever
happened first.** *(Wait mechanism amended by D-K1-20; EINTR and SIGWINCH
registration by D-K1-25.)*

```roc
read! : Tty, U64 => Try(Input, [TerminalErr(IOErr), ..])
Input : [Bytes(List(U8)), Resized, Resumed, TimedOut]
```

Signal handlers write one byte to a self-pipe; `read!` polls the tty fd and
the pipe. No thread, and nothing done in a handler beyond `write(2)`. Timeout
is milliseconds like trantor-net's; **`0` means poll and return**, a deliberate
difference from trantor-net's rejected zero connect budget, because here zero
has a meaning. `Resized` carries no size — a burst collapses into one `size!`.

Because a lone ESC and the start of `ESC [ A` are indistinguishable until more
bytes arrive or do not, the decoder is a state machine, not a function over
bytes: `feed : Decoder, List(U8) -> (Decoder, List(Input))` and
`flush : Decoder -> (Decoder, List(Input))`, called on `TimedOut`. The
batteries' `next_event!` uses an internal escape timeout of ~25 ms.

Rejected: separate `poll_resize!`/`poll_resumed!` (a missed `Resumed` draws
into a cooked shell); a driver-owned callback loop (reshapes the CLI driver).

**D-K1-7 — the resource is the fd; drop closes it and never restores.**
*(A borrowed variant added by D-K1-21; the record's fields by D-K1-22.)*
`TerminalDevice.Tty` is a `[[resources]]` entry whose destructor is `close`.
Refcounting drops at *last use*, not end of scope: a drop-restores design would
put the snake back into cooked mode mid-game once its handle went unused while
it kept reading `Stdin.bytes!`. Restoration belongs to the guard, which is keyed
to the device and process-global — one saved original, one installation,
however many handles.

The batteries `Terminal` is a Roc record `{ tty, decoder, pending }` threaded
through return values. The `Tty` shim takes no handle and uses a lazily opened
process-wide one; that is the package's only global and it exists for
compatibility.

Rejected: no handle at all, with pending escape bytes held host-side behind an
`unread!`. It ties layer e to the host and makes `NoTerminal` a possibility at
every call instead of at `open!`.

**D-K1-8 — `Cooked`, `Cbreak`, `Raw`; scopes restore what they found.**
*(`with_mode!`'s error row as written below does not parse; D-K1-22 has the
measured form.)*

| Mode | ICANON | ECHO | ISIG | OPOST |
|---|---|---|---|---|
| `Cooked` | on | on | on | on |
| `Cbreak` | off | off | on | on |
| `Raw` = `cfmakeraw` | off | off | off | off |

`Raw` matches upstream exactly (the snake's `\r\n` depends on it). `Cbreak` is
what prompts and pickers want: keys immediately, Ctrl-C still kills, `\n` still
works. VMIN/VTIME are not exposed; `read!`'s timeout subsumes them.

```roc
Terminal.with_mode! : Terminal, Mode, (Terminal => Try((Terminal, a), e))
                      => Try((Terminal, a), [ModeFailed(IOErr)]e)
```

A scope restores the mode *it found*, so a `Cbreak` prompt inside a `Raw` app
returns to `Raw`. Only the guard knows the shell's original. The mode is
restored on `Ok` and `Err`; a restore failure is reported only if the body
succeeded, so it never masks the app's error. No `set_mode!` in the batteries.

**D-K1-9 — the host holds an opaque enter/exit byte pair.**
Termios is half of what leaks. Alt screen, hidden cursor, mouse reporting,
bracketed paste and kitty keyboard flags are set by escape sequences and leak
just as badly on a crash.

```roc
set_restore! : Tty, { enter : List(U8), exit : List(U8) } => Try({}, [TerminalErr(IOErr), ..])
```

Teardown writes `exit`, then restores termios (escape first: leaving the alt
screen while still raw is harmless, the reverse can echo junk). SIGCONT
re-enters the mode and writes `enter`. Layer f's scoped helpers push their pair
onto a stack and re-register the concatenation, with D-K1-8's nesting. The host
never learns what the bytes mean, so a replacement output layer registers its
own.

Signal safety: the handler only `write(2)`s a buffer it reads lock-free.
Registration builds a new buffer and swaps it in atomically; the old one is
leaked, never freed, because a handler may be mid-read. Bounded by scope
changes. Capped at 4096 bytes; over the cap is an error, not a truncation.

Rejected: a host that knows a fixed feature list (`enable_alt_screen!` …). It
makes layer f unreplaceable for exactly the state that must be restored.

**D-K1-10 — add-on package over trantor-cli; one component per
replaceable layer.**

| Component | Kind | Exports | Layer |
|---|---|---|---|
| `terminal-host` | rust | interface `terminal` → `TerminalDevice`, resource `Tty` | a–d |
| `terminal-keys` | roc | `Keys` | e |
| `terminal-ansi` | roc | `Ansi` | f1 |
| `terminal-width` | roc | `Width` | f (D-K1-13) |
| `terminal-screen` | roc | `Screen` | f2 |
| `terminal-lib` | roc | `Terminal` | batteries |
| `tty-shim` | roc | `Tty` | basic-cli compat |

Depending on trantor-cli makes `TerminalErr(IOErr)` the same nominal the rest
of an app uses, which is why trantor-net depends on it. A non-CLI world cannot
take this package alone; judged acceptable.

`TerminalDevice`, not `TerminalHost`: the `*Host` suffix was pushed back on
earlier in trantor-cli's cleanup, and "device" says what the module is. `Tty`
stays the shim's because basic-cli apps import that name.

Two ways to replace, both documented: a same-named component with the same
module name and signatures (because `Terminal` imports it), or skipping
`terminal-lib` for `TerminalDevice` directly when the replacement has a
different shape.

Rejected: one Roc component for e+f+batteries (nothing below the whole package
is swappable); standalone with its own error type (every app maps two `IOErr`s).

*(D-K1-11's handler order and "behaves as if absent" claim are superseded by
D-K1-19; libc's use widens by D-K1-19 and D-K1-23.)*

**D-K1-11 — `rustix` + `signal-hook` + `libc` for `atexit`
alone, pinned, with a no-`cc` gate.**

- `rustix`: `tcgetattr`/`tcsetattr`/`tcgetwinsize`/`isatty`/`poll`, and
  `rustix::pty` for the test harness.
- `signal-hook` (+ `-registry`): chaining, `low_level::emulate_default_handler`,
  the self-pipe. Hand-rolled chaining is where `SA_SIGINFO`-vs-plain, `SIG_IGN`
  and re-raise bugs live.
- `libc`: `atexit` only. Already in the tree through signal-hook-registry, and
  trantor itself calls libc for SIGPIPE on the same reasoning. `dtor` was
  considered — it is the community's answer — and rejected as two crates
  (it pins a proc-macro) for the same single `extern` call.

The user's constraint was *no C compiler*. It is true of this set, and made a
gate rather than an assumption: `cargo tree -e build,normal -i cc` must not
match, with a positive control that enables `extended-siginfo-raw` and must
fail.

Handler order, corrected from D-K1-3 by reading signal-hook's source: **previous
handler → teardown → default-action emulation**. A recording handler such as
seahaven's runs first and is unaffected. A previous handler that `_exit`s itself
means teardown never runs; documented, not designed around, since running first
would mean replacing rather than chaining. When no terminal state is active the
action only emulates the default, so a program that opened a terminal and never
changed it behaves as if the package were absent.

**D-K1-12 — one `Input` type; legacy ambiguities resolve to the
named key; kitty always decoded, opt-in enabled.**

```roc
Input : [
    Key({ key : Key, mods : Mods, action : [Press, Repeat, Release] }),
    Mouse({ button : [Left, Middle, Right, WheelUp, WheelDown, None],
            action : [Press, Release, Move, Drag], col : U16, row : U16, mods : Mods }),
    Paste(List(U8)),
    Focus([Gained, Lost]),
    Reply(Reply),
    Unknown(List(U8)),
]
Key : [Char(Str), Enter, Tab, Backspace, Esc, Up, Down, Left, Right,
       Home, End, PageUp, PageDown, Insert, Delete, F(U8)]
Mods : { shift : Bool, ctrl : Bool, alt : Bool, super : Bool }
```

- `Char(Str)` holds exactly one scalar so `Char("q")` matches; invalid UTF-8 is
  `Unknown`, never replaced. `Char(U32)` rejected: every match becomes a number.
- Legacy: `0x09` → `Tab`, `0x0d` → `Enter`, `0x7f` and `0x08` → `Backspace`,
  other C0 → `Char` + `ctrl`. A limit of the protocol, documented.
- Kitty `CSI u` is always understood; enabling it is `Ansi.with_kitty_keyboard!`.
  Without it `action` is always `Press`. Legacy-only rejected: no `Release`
  ever, which games need, and Ctrl-I/Tab ambiguous forever.
- SGR mouse (`?1006`) only; X10 cannot address columns past 223.
- `Paste` is bytes. `Unknown` and unsolicited `Reply` are surfaced, never
  dropped. Positions are 0-based, converted from SGR's 1-based, to agree with
  `size!`.

`Reply` was added at D-K1-14.

**D-K1-13 — three components: sequences, width, screen.**

- `Ansi`: pure sequence builders (cursor, erase, SGR with 16/256/truecolor, alt
  screen, mouse, paste, kitty, synchronized output) and the D-K1-9 scoped
  helpers.
- `Width`: `Width.of : Str -> U16` and grapheme segmentation, tables generated
  from a **pinned Unicode version** by a script that regenerates and diffs.
  UAX #11 plus emoji presentation. Where a terminal disagrees on an emoji's
  width the output misaligns, the trade ratatui and notcurses make; `Width` is
  its own component so a world can choose differently.
- `Screen`: `Cell : { grapheme : Str, style : Style }`, draw into a grid,
  `flush!` diffs against the previous frame and writes the frame in one
  `write!`, wrapped in synchronized output (`?2026`) when supported.

Colour depth `[None, Ansi16, Ansi256, TrueColor]` is a parameter to `Ansi` and
`Screen`, which downsample. Detection (`NO_COLOR`, `COLORTERM`, `TERM`, via
trantor-cli's `Env`) lives in `Terminal`, so the pure layers stay pure.

Rejected: sequences only (every app hand-tracks the cursor and flickers);
screen without width tables (misaligned at the first CJK or emoji).

**D-K1-14 — `Reply` joins `Input`; queries use the DA1 sentinel;
`Terminal` holds a pending queue.**

Cursor position (`CSI 6n`), kitty flags (`CSI ? u`), background colour
(`OSC 11;?`) and mode support (`DECRQM`, e.g. `?2026`) are answered in the input
stream, and an unsupporting terminal answers *nothing*. Every terminal answers
Primary DA (`CSI c`), so a query is written followed by DA1: the matching reply
means `Answered`, DA1 first means `Unsupported`, neither before the timeout
means `TimedOut`. notcurses and crossterm do the same.

```roc
Reply : [CursorPos({ col : U16, row : U16 }), KittyFlags(U8), DeviceAttributes(List(U16)),
         Background({ r : U16, g : U16, b : U16 }), Mode({ mode : U16, state : U8 })]
Terminal.query! : Terminal, Query, U64
                  => Try((Terminal, [Answered(Reply), Unsupported]), [TimedOut, TerminalErr(IOErr), ..])
```

Input arriving during a query goes to `Terminal.pending`; `next_event!` drains
it first. `open!` probes nothing — `--color auto` should not pay a round trip.
`Screen` asks about `?2026` once on first use; `with_kitty_keyboard!` asks
before enabling. Rejected: no queries, which leaves a game unable to know
whether `Release` will ever arrive.

**D-K1-15 — `size!` returns `{ cols : U16, rows : U16 }`; 0×0 is `UnknownSize`.**
Some serial consoles report zero. No pixel size. [ASSUMPTION stated during the
grill, not objected to.]

**D-K1-16 — a pty harness carries the guarantees; every gate has
a positive control.** The package is gated by `trantor test .` (T3): trantor-cli
as `[dev-deps]`, `expect`s, README examples, and `tests/<n>/`. The harness is a
Rust crate on `rustix::pty` driven from a `test.sh`, holding its own slave fd
until the master is drained. Positive controls are mutations: the `test.sh`
copies the package into `$TMP`, patches one line, composes that, and requires
the affected rows to fail — no test-only feature or env var in the shipped host.

| Type | Where | Cases |
|---|---|---|
| Unit (`expect`, step 4 of `trantor test .`) | `terminal-keys` | ~40: each legacy key and ambiguity; kitty incl. `Release`; SGR mouse and the 1→0 conversion; paste split across feeds; ESC+flush → `Esc`; ESC+`x` → Alt; invalid UTF-8 → `Unknown`; each `Reply` |
| Unit | `terminal-width` | width samples for CJK, combining, ZWJ, VS16 |
| Golden (`main.roc` + `expected`) | `tests/graphemes/` | the pinned `GraphemeBreakTest.txt` in full |
| Unit | `terminal-ansi` | sequence bytes; downsampling at boundaries |
| Unit | `terminal-screen` | frame pairs → bytes: identical → empty; one cell; wide over narrow; resize |
| Integration (pty) | `tests/pty/` (`test.sh`) | modes via harness-side `tcgetattr`; every row of the exit table, each asserting termios restored *and* exit bytes written in order; chaining; `TIOCSWINSZ` → `Resized` → `size!`; `read!(0)` immediate; `NoTerminal` under `setsid`; query answered / unsupported / timed out, with keys mid-query delivered |
| E2E | b8 | `tty` and `terminal-app-snake` roc-check by URL swap (floor 18 → 20); snake run over the pty with scripted keys, terminal restored afterwards |
| Build gate | `tests/no-cc/` (`test.sh`) | no `cc` in the tree |

Positive controls: `atexit` registration stubbed must fail the exit rows;
chaining disabled must fail the chaining row; `extended-siginfo-raw` enabled
must fail the `cc` gate; the grapheme check requires >1000 vectors.

Not proposed: snapshotting real emulators (tmux, xterm) — needs them installed,
and tests them more than this package.

**D-K1-17 — three phases, each green and committed alone.** Device and guard
first (the only unsafe code, and useful alone: the upstream examples return,
rocjust's `is_terminal!` gets a home); input second; output third, because
`Screen`'s `?2026` probe uses `query!`. New sibling repo
`~/dev/roc/trantor-terminal`, like trantor-net. ID `k1` because `t` is
temporal's.

## Plan details accepted (2026-09-12)

**D-K1-18 — seven details the plan added beyond the grill, accepted as a set.**

1. A `mode!` leaf: `with_mode!` must read what it will restore to. It reports
   what the guard last set, not a classification of arbitrary termios.
2. SIGINT is handled like SIGTERM: Cbreak keeps ISIG on, so without it
   "Ctrl-C still kills, the guard restores" (D-K1-8) would be false.
3. A `shared!` leaf holds the `Tty` shim's process-wide handle — the package's
   one global.
4. The guard writes teardown bytes to its own never-closed `/dev/tty` fd, so a
   dropped `Tty` cannot close the fd a handler writes to.
5. `Event` wraps `Keys.Input` (`[Input(Keys.Input), Resized, Resumed,
   TimedOut]`) so a replacement decoder does not force a `Terminal` change.
6. `Terminal` gains the restore stack and a sync-support cache.
7. The Unicode table generator is a Rust binary, and a missing
   `emulate_default_handler` is raised rather than hand-written.

## Adversarial review (2026-09-12)

An independent reviewer, told to measure on this Mac rather than reason where it
could, found 1 blocker, 6 major, 6 minor. Probes are in the session scratchpad
(`k1-review/probe`, `k1-review/roc`). Every recommendation was accepted.

**D-K1-19 — the guard leaves claimed signals alone.** Measured: with
`SIG_IGN` on SIGHUP, `raise` survives (rc 0); add `register_unchecked` +
`emulate_default_handler` and it exits 129. A recording SIGINT handler followed
by the guard exits 130. signal-hook-registry's `Prev::execute` skips `SIG_IGN`
(`lib.rs:268`), and emulating the default then kills. That broke `nohup`,
background jobs in non-interactive shells, and the reason seahaven's `Signal`
exists ("finish what it is doing before it dies").

So at install the guard queries each fatal signal's disposition read-only with
`sigaction` and registers only those still at `SIG_DFL`. A claimed or ignored
signal does not kill the process; whenever it exits, `atexit` restores.
Registered-but-inactive emulates the default, which is now exactly the absent
behaviour, because registration only happened where the default was in force.
This supersedes D-K1-11's "previous handler → teardown → default" for claimed
signals, and its `_exit`-in-a-prior-handler caveat no longer arises.

The fatal set widens to every catchable signal whose default terminates or
stops: SIGTERM, SIGHUP, SIGQUIT, SIGINT, SIGPIPE, SIGALRM, SIGUSR1/2, SIGXCPU,
SIGXFSZ, SIGVTALRM, SIGPROF, SIGTSTP. SIGPIPE matters most: the driver's C
`main` leaves it at the default, so `tui | head` dies of it. SIGSEGV (a Roc stack
overflow — no Rust overflow handler is installed under a C `main`) is forbidden
by the registry and is documented as `reset` territory.

**D-K1-20 — `select` on Apple, `poll` elsewhere.** Measured under a pty with
the child as session leader: `poll` on the `/dev/tty` fd returns
`revents=0x20` (POLLNVAL) at once, with or without input; `poll` on fd 0 works;
`select` on the `/dev/tty` fd works. Polling fd 0 instead would give up D-K1-5's
piped-stdin case, so the wait is per-platform and `/dev/tty` stays. Whether
Darwin's `select` accepts an fd ≥ `FD_SETSIZE` is to be measured at
implementation and recorded here; a pty row forces a high fd.

**D-K1-21 — `Tty` has a borrowed variant.** The resource ABI has `new`, `get`,
`release`, `with` and no retain (`src/codegen.rs`), and the last Roc drop runs
the boxed value's destructor. A `shared!` that returned an owning `Tty` would
close the one shared fd at the first handle's last use, and a later
`disable_raw_mode!` would `tcsetattr` a closed or reused fd — with the error
swallowed by the shim's infallible signature. The host value is
`Owned(OwnedFd) | Shared(RawFd)`; `shared!` returns the process-wide fd (the
same one the guard writes to, D-K1-18 item 4) as `Shared`.

**D-K1-22 — corrections to the Roc surface, all measured against the roc in
use.**
- A nominal record's fields are not readable from another module ("This is not
  a record … It is: Term"), so `Ansi` and `Screen` could not touch
  `Terminal.restore` or the sync cache. `Terminal` exports `push_restore!`,
  `pop_restore!`, `sync_supported`, `with_sync_supported`.
- `[ModeFailed(IOErr)]e` does not parse, and `[..e]` on the body with a wider
  result does not unify. The form that checks is `[ModeFailed(IOErr), ..e]` on
  both the callback and the result.
- `pending : List(Keys.Input)` could not hold a `Resized` or `Resumed` arriving
  during the escape wait or a query; it is `List(Event)`.

**D-K1-23 — teardown cannot be skipped by a nested signal; the harness drains
from spawn.** The registry installs with an empty `sa_mask`, so a different
signal can interrupt teardown; an "already torn down" flag would then skip
`tcsetattr` and die raw. Teardown writes the exit bytes at most once per
activation but calls `tcsetattr(original)` unconditionally; `atexit` teardown
blocks the fatal set with `pthread_sigmask`; SIGCONT clears the flag after
re-entering. `libc` now also supplies `sigaction` (query only) and
`pthread_sigmask` — declarations, no C.

Measured on macOS: a pty child that has written output stays in exit (`?NEs`)
until the master is read, and a harness blocked in `wait4` hangs. The harness
drains the master on a thread from spawn.

**D-K1-24 — a `suspend!` leaf.** In `Raw`, ISIG is off, so Ctrl-Z arrives as
`0x1a` and nothing raises SIGTSTP. `suspend!` raises it through the guard, as
vim and htop do. If SIGTSTP is claimed or ignored (D-K1-19), it goes to the
claimant.

**D-K1-25 — SIGWINCH is registered at `open!`, and `read!` retries EINTR.** A
Cooked app that only reads and asks for the size would otherwise never see
`Resized`. `poll`/`select` return EINTR under a handler regardless of
`SA_RESTART`, so `read!` retries with the remaining budget from a monotonic
clock.

On Linux, socket reads under `SO_RCVTIMEO` return EINTR despite `SA_RESTART`,
so any installed handler — this package's or anyone's — surfaces a resize as
`Interrupted` from trantor-net's `Tcp` (`sockets-host/src/lib.rs:183`). That is a
trantor-net defect and is fixed there, not worked around here.

*(Fixed in trantor-net f2c71eb, `tests/eintr`.)* Measured on Linux 6.8: TCP
recv, UDP recv and TCP send all return EINTR under a handler, with or without
`SA_RESTART`. Each socket leaf now takes one monotonic deadline for the whole
call and retries EINTR within it. On macOS the kernel behaves differently:
without `SA_RESTART` it returns EINTR, which the fix covers, but with
`SA_RESTART` it restarts the call with a fresh timeout (2.71s for a 2s budget),
and no host retry can bound that. So on macOS, the flags this package installs
SIGWINCH with decide whether a resize can stretch a trantor-net call — decided
by D-K1-28.

**D-K1-26 — `TCSANOW` for mode entry, not `TCSAFLUSH`.** Flushing discards keys
typed during startup (the snake enables Raw before drawing), and made the b8
snake run racy: keys sent before Raw were thrown away and the loop blocked
forever. crossterm uses `TCSANOW`. Test harnesses send keys only after a
readiness marker.

Minor corrections folded into the plan without a decision: rustix's `pipe`
feature; a nonexistent `Query.DeviceAttributes` reference removed; b8 asserts
`tty` and `terminal-app-snake` by name because the floor alone does not prove
inclusion; `GraphemeBreakTest.txt` added to the checked-in UCD files; the
`inactive` pty row (which a binary without the package also passed) replaced by
installed-inactive, inherited-ign, prior-handler, read-eintr, resize-cooked,
suspend, sig-pipe and select-high-fd rows, with positive controls for the
disposition check and the EINTR retry.

Verified sound by the reviewer: registry order; `cc` only behind
`extended-siginfo-raw`; ENXIO opening `/dev/tty` with no controlling terminal;
the driver's exit paths; rustix 1.1.4 providing every termios and pty call
needed on macOS; `{ super : Bool }` and `() => {}` checking; the `tests/pty/`
layout as a single-kind T3 directory.

## Implementation (2026-09-12)

**D-K1-27 — Width's Unicode data comes from ICU4X's compiled data, not from
downloaded UCD files.** When implementation reached `Width`, only
`GraphemeBreakTest-17.0.0.txt` existed locally (in the cached `icu_segmenter`
2.3.0). The plan's source — four UCD .txt files checked in — needed a download
from unicode.org; the alternative was `icu_properties` 2.3.0, already cached,
whose compiled data is Unicode 17.0.0 and carries every property needed
(East_Asian_Width, General_Category, Default_Ignorable_Code_Point,
Emoji_Presentation, Grapheme_Cluster_Break, Extended_Pictographic,
Indic_Conjunct_Break). The user chose ICU4X. The cost: the check becomes
"matches this crate version" rather than "matches the Unicode files"; the
conformance file is still Unicode's own, so a wrong table still fails it.

Two findings from implementing, both in the plan's record: rustix 1.1.4's Apple
`select` produces `tv_usec = 1_000_000` for a budget just under a whole second
(EINVAL); and the guard must not touch the terminal from outside its
foreground process group, where `tcsetattr` is answered with SIGTTOU.

**D-K1-28 — every guard signal, SIGWINCH included, keeps `SA_RESTART`.**
`signal_hook_registry::register_sigaction` installs with `SA_RESTART` (1.4.8,
`src/lib.rs:187`): one process-wide handler per signal, shared by every
registrant. `guard.rs` registers SIGWINCH through it, and that stays. The macOS
cost of keeping it is trantor-net's to fix, as D-K1-25's Linux defect was.

Measured on macOS (Darwin 25.3) and Linux 6.8 (colima). Unless stated, one
signal 1s into a 2s budget:

| Call | No `SA_RESTART` | `SA_RESTART` |
|---|---|---|
| macOS UDP recv under `SO_RCVTIMEO`, SIGWINCH every 0.5s for 8s | EINTR at 0.50s | EAGAIN at **10.03s** |
| macOS `poll`, the same signals | — | EINTR at 0.50s |
| ureq 3.4.0 GET (http-host's client), macOS | `Interrupted` at 1.01s | `Timeout` at 3.00s |
| the same, Linux | `Interrupted` at 1.01s | `Interrupted` at 1.00s |

Without `SA_RESTART`, every blocking call in the process that does not retry
EINTR fails on every resize, on both platforms. trantor-cli's `Streams.read!` is
a plain `read` (`components/sync-io/src/lib.rs:49`), so a Cooked app reading
stdin or a subprocess pipe would break on a resize — the very program D-K1-25
registers SIGWINCH at `open!` for. So would ureq, and any C library linked in.
Nor can it be had through the registry: clearing the flag after registering
changes it for every registrant of that signal, and bypassing the registry gives
up the chaining D-K1-11 chose it for.

With `SA_RESTART`, EINTR stays confined to calls POSIX never restarts: `poll`
and `select`, which `read!` already retries (D-K1-25), and Linux's timed
sockets, which trantor-net `f2c71eb` retries. What remains is macOS stretching a
timed socket call for as long as resizes keep arriving: a window drag held past
the budget holds the call open. No retry around an `SO_RCVTIMEO` call can bound
that. A deadline enforced by `poll` can, because `poll` returns EINTR under
`SA_RESTART` on macOS.

Rejected: installing without `SA_RESTART`. It buys exact trantor-net budgets on
macOS today, in a process where every other blocking read breaks on a resize.

## Still open (raised, not decided)

- rocjust's migration to `trantor-terminal` for `Tty.is_terminal!` is not part
  of K1.
- `Stdout` writes interleaved with `Screen` frames on the same device are the
  app's to avoid; whether `Terminal` should warn is not decided.
- trantor-net, from D-K1-28: sockets-host should wait on `poll` against its
  deadline instead of `SO_RCVTIMEO`/`SO_SNDTIMEO`, so macOS budgets hold under
  `SA_RESTART`.
- trantor-net, from D-K1-28: http-host still has D-K1-25's Linux defect. ureq
  3.4.0's `TcpTransport::await_input` is a plain `read` under
  `set_read_timeout` with no EINTR retry
  (`src/unversioned/transport/tcp.rs:217–231`), so a resize fails an HTTP
  request with `Interrupted` whatever the flags.
