# Just recipes for build actions
alias bin := release
alias old := outdated

all: build examples release

bench:
    cargo bench
    target/release/bench-table

build:
    cargo build --all-targets && cargo clippy

examples:
    cargo build --examples
    cargo build -p bench-table --release

outdated:
    cargo outdated --depth=1

release:
    cargo build --release

test:
    cargo nextest run --test-threads num-cpus

test-out:
    cargo nextest run --no-capture --test-threads num-cpus

test-rel:
    cargo nextest run --release --test-threads num-cpus

tests: test test-rel
