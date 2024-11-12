# https://github.com/casey/just
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

alias r := run
alias b := build
alias br := build-release
alias rr := release
alias rel := release
alias t := test
alias ti := test-integration
alias c := clippy

run *ARGS:
  cargo run {{ARGS}}

build *ARGS:
  cargo build {{ARGS}}

release:
  cargo run --release --features portable

build-release:
  cargo build --release --features portable

test *ARGS:
  cargo test {{ARGS}}

test-integration *ARGS:
  cargo test -- --ignored --nocapture --test-threads=1 {{ARGS}}

clippy:
  cargo clippy --all-targets --all-features
