# Icemelter

Icemelter is a tool to minimize Rust files that trigger internal compiler
errors (ICEs).

## Usage

Icemelter works on standalone Rust files. If your file is named `ice.rs`, use
it like so:

```sh
icemelter ice.rs
```

By default, the result is stored to `melted.rs` (this can be changed with
`--output`).

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

Icemelter is based on [`treereduce-rust`][treereduce].

[cargo]: https://doc.rust-lang.org/cargo/
[crates-io]: https://crates.io/
[releases]: https://github.com/langston-barrett/icemelter/releases
[rustup]: https://rustup.rs/
[treereduce]: https://github.com/langston-barrett/treereduce
