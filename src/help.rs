//! `trantor --help`.
pub const HELP: &str = "\
trantor — compose Roc platforms from Rust components.

Starting out
  new <dir> [--from <path|org/repo>]  scaffold a project (and compose it)
  add <org/repo> [<dir>] [--as <name>] add a dependency here (package or world),
                                      pinning a semver tag
  update [<name>] [<dir>]             move a pin (--all [<dir>]: every pin)
  remove <name> [<dir>]               drop a dependency
                                      add, update and remove compose afterwards and
                                      keep the change only if that succeeds

Working
  check <dir>                         compose + typecheck (the inner loop)
  run <dir> [-- <args>]               build, then run the app
  test <dir>                          a world: roc expects + cargo tests
                                      a package: composed on its [dev-deps], plus tests/
  build <dir> [--app <d>] [--out <n>] [--target <t>] [--platform-only]

Adding a capability
  new-interface <dir> <name>          scaffold an interface + host component
  interface-stub <dir> <name>         the Rust signatures, from the Roc

Platform authoring
  compose <dir> [--out <d>]           generate sources only
  scan <dir>                          the archive symbol-collision scan
  publish <dir>                       package a baseline
  tier <dir>                          classify an extension

Common flags: --world <file> picks a world variant (default world.toml).

Testing a package (a directory with package.toml, run as `trantor test .`)
  Composes it alone (it must fail naming the driver unless one is in reach),
  then on its [dev-deps]; checks the dev-deps alone expose none of its
  exports; runs its expects; builds README.md's roc blocks (a whole app runs
  as written) and compares each `expr   # value` comment; then each
  tests/<name>/ is one of:
    main.roc + expected   an app, stdout compared line for line
    Cargo.toml            cargo test --release
    test.sh               run with TRANTOR ROC PKG TMP DEPS DEV_DEPS
  A README example's missing bindings come from tests/readme-prelude.roc.
";
