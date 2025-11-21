# Just recipes for build actions
alias lib := release
alias doc := documentation
alias old := outdated

all: build examples release

bench: table
    cargo +nightly bench --features petgraph,bench-nightly
    target/release/bench-table

build:
    cargo build && cargo clippy

dijkstra:
    cargo run -p dijkstra --release

documentation:
    cargo doc --no-deps --open

examples:
    cargo build --examples --release
    cargo build -p dijkstra --release

outdated:
    cargo outdated --depth=1

release:
    cargo build --release

serde:
    cargo test --test serde --features serde

table:
    cargo build -p bench-table --release

test:
    cargo nextest run --test-threads num-cpus --features serde

test-out:
    cargo nextest run --no-capture --test-threads num-cpus
