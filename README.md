# Icemelter

Icemelter automates steps in debugging rustc internal compiler errors (ICEs).

## Features

- Automatically minimizes files that cause the ICE (MCVEs)\* 
- Runs [cargo-bisect-rustc][cargo-bisect-rustc]
- `rustfmt`s MCVEs if doing so keeps the ICE
- Generates copy-pasteable Markdown reports
- Optionally downloads MCVEs from Github

\*It really works: Icemelter reduced a ~250 line file to just 4 lines in [#107454][#107454]).

More features are [planned][issues].

[#107454]: https://github.com/rust-lang/rust/issues/107454
[cargo-bisect-rustc]: https://github.com/rust-lang/cargo-bisect-rustc
[issues]: https://github.com/langston-barrett/icemelter/issues

## Usage

Icemelter works on standalone Rust files. If your file is named `ice.rs`, use
it like so:

```sh
icemelter ice.rs
```

By default, the result is stored to `melted.rs`. A few helpful flags:

- `--output`: Change where the output file is written
- `--bisect`: Bisect the regression with cargo-bisect-rustc
- `--markdown`: Output a copy-pasteable report

Here's an example that uses a different compiler and adds a flag:

```sh
icemelter ice.rs -- rustc +nightly --crate-type=lib
```

For more options, see `--help`.

## Installation

### From a release

Statically-linked Linux binaries are available on the [releases page][releases].

### From crates.io

You can build a released version from [crates.io][crates-io]. You'll need the
Rust compiler and the [Cargo][cargo] build tool. [rustup][rustup] makes it very
easy to obtain these. Then, to install the reducer for the language `<LANG>`,
run:

```sh
cargo install icemelter
```

This will install binaries in `~/.cargo/bin` by default.

## Build

To build from source, you'll need the Rust compiler and the [Cargo][cargo] build
tool. [rustup][rustup] makes it very easy to obtain these. Then, get the source:

```sh
git clone https://github.com/langston-barrett/icemelter
cd icemelter
```

Finally, build everything:

```sh
cargo build --release
```

You can find binaries in `target/release`. Run tests with `cargo test`.

## How it works

Icemelter's minimization capabilities are built on
[`treereduce-rust`][treereduce].

[cargo]: https://doc.rust-lang.org/cargo/
[crates-io]: https://crates.io/
[releases]: https://github.com/langston-barrett/icemelter/releases
[rustup]: https://rustup.rs/
[treereduce]: https://github.com/langston-barrett/treereduce
