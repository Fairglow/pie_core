# Just recipes for build actions
alias doc := documentation
alias old := outdated

all: lint build examples table test

bench: table
    unbuffer cargo +nightly bench --features petgraph,bench-nightly | tee bench.log
    unbuffer target/release/bench-table | tee -a bench.log

build: lint
    unbuffer cargo build --all-targets | tee build.log

build-all: build examples release

dijkstra:
    cargo run -p dijkstra --release

documentation:
    cargo doc --no-deps --open

examples:
    unbuffer cargo build --examples --release | tee examples.log
    unbuffer cargo build -p dijkstra --release | tee -a examples.log

gemini:
    npx @google/gemini-cli

lib:
    unbuffer cargo build --lib | tee build.log

lint:
    unbuffer cargo clippy | tee lint.log

outdated:
    cargo outdated --depth=1

release:
    cargo build --release

serde:
    cargo test --test serde --features serde

table:
    cargo build -p bench-table --release

test:
    unbuffer cargo nextest run --test-threads num-cpus --features serde | tee test.log

test-out:
    unbuffer cargo nextest run --no-capture --test-threads num-cpus | tee test.log
