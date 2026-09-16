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

*(Signature superseded by D-K1-32.)*

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
*(For SIGWINCH, made moot by D-K1-29: no thread that blocks ever receives it.)*
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

*(sockets-host done in trantor-net `2e463bb`.)* A socket is now nonblocking for
the length of a leaf call and waits in `poll` against the call's deadline.
`tests/eintr` and `tests/eintr-host` run the `SA_RESTART` half on macOS too. With
the previous `SO_RCVTIMEO` code, macOS's TCP read, read_until and UDP recv under
`SA_RESTART` fail those tests at ~3.0s.

**D-K1-29 — SIGWINCH is received on a thread of its own, and blocked on the
thread that opens the terminal.** D-K1-25 and D-K1-28 left every blocking call
in the process to cope with a resize: sockets-host got a retry and was then
rewritten around `poll` for macOS (`2e463bb`), and http-host needed its own TCP
transport under ureq. That list only grows. The signal should not reach those
calls at all.

At `open!`, `listen_for_resize` blocks SIGWINCH on the calling thread and starts
one thread that blocks every signal except SIGWINCH and parks for the life of
the process. The registry's handler runs there and writes `W` to the self-pipe,
so `read!` wakes as before. The receiver blocks everything else so it never
takes a signal the app thread would have taken; SIGTERM and SIGCONT land where
they did.

Measured with `spikes/k1-sigwinch-thread` on macOS (Darwin 25.3) and Linux 6.8
(colima). One SIGWINCH sent with `kill(getpid())`, as a resize is, 1s into a 2s
budget; every case counted one delivery:

| Call | Handler | Nothing blocked, macOS | Nothing blocked, Linux | Blocked, receiver thread, both |
|---|---|---|---|---|
| ureq 3.4.0 GET, server never answers | `SA_RESTART` | `Timeout` at **3.00s** | **`Interrupted`** at 1.00s | `Timeout` at 2.00s (Linux 2.06s) |
| the same | none | **`Interrupted`** at 1.00s | **`Interrupted`** at 1.00s | `Timeout` at 2.00s |
| std `read` under `SO_RCVTIMEO` | `SA_RESTART` | `WouldBlock` at **3.01s** | **`Interrupted`** at 1.01s | `WouldBlock` at 2.00s |
| the same | none | **`Interrupted`** at 1.00s | **`Interrupted`** at 1.01s | `WouldBlock` at 2.00s |
| pipe `read`, no timeout, byte at 1.5s | `SA_RESTART` | `Ok` at 1.50s | `Ok` at 1.50s | `Ok` at 1.50s |
| the same | none | **`Interrupted`** at 1.00s | **`Interrupted`** at 1.00s | `Ok` at 1.50s |

The handler ran on the receiver every time. A thread that existed before the
block and made the call instead took the signal in 20 of 20 runs per row on
both platforms, with the unblocked results.

The block is at `open!`, not at driver startup. No shipped host starts a thread
(trantor-cli, trantor-net, trantor-terminal and trantor-temporal searched; only
the test-only `testnet-host` does), and the app runs on the driver's main
thread, so at `open!` that thread is normally the only one. Driver startup would
cover a thread started earlier, but it would block SIGWINCH in every trantor
app and every child those apps start, most of which never open a terminal, and
the driver has no hook a package can use. The rule this leaves: a host that
starts a long-lived thread before `open!` and blocks on it must block SIGWINCH
there.

Children inherit the mask. std's `Command` keeps it on purpose
(`library/std/src/sys/process/unix/unix.rs`), and a child started from the
blocked thread reported SIGWINCH blocked on both platforms; with a `pre_exec`
that unblocks, it reported it unblocked. Linux's `/bin/sh` showed an empty mask,
but dash clears its own at startup, which says nothing about vim. So trantor-cli's
subprocess-host gives every child an empty signal mask. A Roc app cannot block
a signal, so whatever is blocked is some host's own business. `pre_exec` moves
std from `posix_spawn` to fork and exec. A child started by code other than
subprocess-host still inherits the block.

What this changes elsewhere:
- http-host needs no transport of its own for resizes.
- sockets-host's `poll` deadlines (`2e463bb`, which replaced `f2c71eb`'s retry)
  stay. Any other handled signal still reaches the app thread.
- SIGWINCH keeps `SA_RESTART`, since the registry sets it, and it no longer
  matters: the receiver makes no call a signal could interrupt.

Rejected: blocking at driver startup, for the reasons above. Unblocking only
SIGWINCH in children: it leaves every other host-blocked signal to leak into
programs that never asked for it.

**D-K1-30 — SIGTSTP and SIGCONT stay on the app thread.** D-K1-29 left the
guard's SIGCONT handler there, and the obvious follow-up was to move it to the
receiver as well. Measured with `spikes/k1-sigwinch-thread` (`--bin stop`): a
separate `sh` stops the process 1s into a 2s budget and sends SIGCONT at 1.3s;
handlers installed as the guard installs them, SIGTSTP emulating its default
with `raise(SIGSTOP)`. Three runs per cell, all alike:

| Setup | ureq GET or timed socket read, macOS | the same, Linux 6.8 | pipe read, both |
|---|---|---|---|
| no handlers, SIGSTOP | `Timeout` at 2.00s | **`Interrupted`** at 1.31s | `Ok` at 1.50s |
| as built by D-K1-29 | `Timeout` at **3.33s** | **`Interrupted`** at 1.31s | `Ok` at 1.50s |
| SIGCONT on the receiver | `Timeout` at **3.33s** | **`Interrupted`** at 1.31s | `Ok` at 1.50s |
| SIGCONT and SIGTSTP on the receiver | `Timeout` at 2.00s | **`Interrupted`** at 1.31s | `Ok` at 1.50s |

Moving SIGCONT alone changes nothing: SIGTSTP's handler runs on the app thread
first and stretches the call. Moving both fixes macOS. Nothing fixes Linux,
which interrupts a timed socket call on resume with no handler installed at all
(`signal(7)` documents it), so a client has to retry there regardless.

Moving both would cost more than it buys. `suspend!` sends SIGTSTP with
`raise`, which is thread-directed and would stay pending on a thread that
blocks it, so it would need `kill(getpid())`. And teardown and resume would run
beside the app thread instead of interrupting it: the app could write a frame
or set a mode between the teardown and the stop, and the enter bytes written on
resume could land in the middle of the app's output. What it buys is one
stretch per Ctrl-Z on macOS, not a stretch for as long as a window drag lasts.

Rejected: SIGCONT alone, which the table shows does nothing; SIGTSTP and
SIGCONT together, for the race in the code that puts the terminal back.

**D-K1-31 — http-host's Linux failure after a stop and resume is documented,
not fixed.** D-K1-30 measured it with no handlers installed: the kernel
interrupts a timed socket read on resume and ureq 3.4.0's `await_input` does not
retry (`src/unversioned/transport/tcp.rs:217–231`), so `send!` returns
`HttpErr(NetworkError)`. It needs no terminal package, only Linux and Ctrl-Z
during a request.

The fix considered was a connector after ureq's `TcpConnector` that wraps its
transport and retries `Interrupted` against the remaining deadline, about 60
lines on ureq's unstable `unversioned` API. Rejected: it puts code on every read
of every request, the hottest path an HTTP app has, for an edge case that
sending the request again recovers from. The bug is ureq's, where the fix is a
loop inside `await_input`; a ureq release that retries is picked up by bumping
the pin. trantor-net's README states the limitation. Tcp and Udp are not
affected: sockets-host waits in `poll` against each call's deadline (`2e463bb`).

**D-K1-32 — `query!` succeeds only with the reply; `Unsupported` and
`TimedOut` are errors that carry the `Terminal`.** D-K1-14's `Err(TimedOut)`
discarded the `Terminal` the query had been reading into, and code review found
two losses. Keys typed while the query waited were in that value's `pending`,
so a caller falling back to the `Terminal` it passed in never saw them. And the
DA1 reply the query wrote was still on its way: it arrived during the next
query, which took it as `Unsupported`, or took the earlier query's late reply
as its own answer.

```roc
Terminal.query! : Terminal, Query, U64
                  => Try((Terminal, Reply), [Unsupported(Terminal), TimedOut(Terminal), TerminalErr(IOErr), ..])
```

A query exists to get an answer, so both ways of not getting one are errors:
`Unsupported` is a terminal that cannot answer, `TimedOut` no terminal or a
broken one. With one success left, the `Answered` wrapper is gone. Both errors
carry the `Terminal` so the program keeps what the query read. That `Terminal`
counts, in `stale_da`, the DA1 replies owed by queries that timed out. Until
each has arrived, a later query takes nothing as its answer, and `next_event!`
drops a DA1 reply while the count is above zero. A budget too large to add to
the clock saturates.

Rejected: `TimedOut` in `Ok` beside `Answered` and `Unsupported`, the first fix
(trantor-terminal `7b027f3`), because a timeout is a failure, not an answer.
Bare `Err(TimedOut)` and `Err(Unsupported)`, which keep both losses.
`with_kitty_keyboard!` and `Screen`'s synchronized-output check treat both
errors as unsupported (trantor-terminal `d02f67f`, `2f3139d`).

**D-K1-33 — `Terminal` does not warn about `Stdout` writes interleaved with
`Screen` frames.** Decided by the user. Keeping `Stdout` and `Screen` from
writing to the same device at once stays the app's job. `Terminal` has no view
of those writes: `Stdout` is trantor-cli's stream, a separate fd that
`Terminal` never wraps.

**D-K1-34 — a restore pair is kept by its bytes and reused.** Decided by the
user after the first independent review. Every pair ever registered stays in a
registry keyed by its enter and exit bytes, and registering bytes seen before
points the handler-visible pointer back at that allocation. The no-free rule
D-K1-9 needs for signal safety is unchanged; what changes is that memory grows
with the number of distinct stacks a program uses, not with the number of
registrations. The registry is behind a plain `Mutex`, touched only by
`set_restore!` on normal threads; handlers still do one atomic load.

The review measured about 64 bytes leaked per registration, so a per-frame
`Ansi.with_pair!` leaked about 27 MB an hour. Skipping a registration whose
bytes match the current pair was tried first and did not help: a scope pushes
one stack and pops to another every frame, so neither ever matches the one
current. Measured after, a 200,000-iteration per-frame scope stays at 1.7 MB
against a 1.5 MB baseline, where it was 27 MB.

Rejected: skipping identical registrations only (the per-frame case never
repeats the current pair); documenting that scopes must not be entered per
frame (the leak stays).

**D-K1-35 — an unterminated paste settles after 1 second of quiet, not 25 ms.**
Decided by the user after the first independent review. A bracketed paste whose
end marker never arrives is delivered as `Paste` of what came, after
`paste_wait_ms` (1 000) with no new input, and is capped at `longest_paste`
(1 MiB). Escapes keep their 25 ms. `Keys` gains `is_pasting : Decoder -> Bool`,
one function on the replaceable component's surface, so `Terminal` can tell the
two waits apart; `Terminal` records when input last arrived so the gap is
measured across `next_event!` calls, each of which keeps its single deadline.

The review found that a paste without its end marker swallowed every later key,
`q` included, with memory unbounded. Settling it after an escape's 25 ms was
tried first; over a slow link a paste pausing longer than that was cut in two
and its tail delivered as keystrokes (measured: a 200 ms pause split
`hello world` into `Paste(hello)`, six key events and `Unknown`).

Rejected: 25 ms (splits pastes over ssh); no settling (the original hang). The
cost accepted: keys typed within a second of a lost end marker join the paste.

Amended after the change review (user, accepting the recommendation). The
quiet gap is only judged after a zero-timeout read finds nothing waiting: the
first version flushed a paste pending across calls before reading, so a
program polling `next_event!(t, 0)`, or busy between calls, had pastes cut into
keystrokes that the code before D-K1-35 delivered whole. And a paste past
`longest_paste` no longer ends paste mode: it arrives as consecutive `Paste`
pieces of at most 1 MiB up to its end marker, so its bytes are never decoded as
input, and memory stays bounded. A program sees one long paste as several
`Paste` events.

Rejected for the cap: dropping bytes past it until the end marker (loses the
paste's content silently).

## Still open (raised, not decided)

- `Screen` caches `Unsupported` for synchronized output when its first query is
  made in Cooked mode, which now answers `Unsupported` without asking. Not
  caching it would need `Screen` to know the mode, a new export. Found while
  fixing the first review's Cooked-mode query finding.

- rocjust's migration to `trantor-terminal` for `Tty.is_terminal!` is not part
  of K1.
