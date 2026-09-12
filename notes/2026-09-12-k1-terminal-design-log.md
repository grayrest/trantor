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
happened first.**

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
| Unit | `terminal-width` | the pinned `GraphemeBreakTest.txt` in full; width samples for CJK, combining, ZWJ, VS16 |
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

## Still open (raised, not decided)

- rocjust's migration to `trantor-terminal` for `Tty.is_terminal!` is not part
  of K1.
- `Stdout` writes interleaved with `Screen` frames on the same device are the
  app's to avoid; whether `Terminal` should warn is not decided.
