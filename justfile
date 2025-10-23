# Just recipes for build actions
alias lib := release
alias doc := documentation
alias old := outdated

all: build examples release

bench: table
    cargo +nightly bench
    target/release/bench-table

build:
    cargo build && cargo clippy

documentation:
    cargo doc --no-deps --open

examples:
    cargo build --examples --release

outdated:
    cargo outdated --depth=1

release:
    cargo build --release

table:
    cargo build -p bench-table --release

test:
    cargo nextest run --test-threads num-cpus

test-out:
    cargo nextest run --no-capture --test-threads num-cpus
