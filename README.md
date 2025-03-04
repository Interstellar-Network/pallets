# pallets

Those are the pallets meant to be used either through(ie as a submodule):
- https://github.com/Interstellar-Network/substrate-offchain-worker-demo
- https://github.com/Interstellar-Network/integritee-worker

It is NOT meant to work as standalone.

## WIP standalone tests

NOTE: those are using Substrate testing framework, not Integritee(if it even exists).

`[RUST_BACKTRACE=1] cargo test [--no-fail-fast] -p pallet-ocw-garble -p pallet-ocw-circuits -p pallet-tx-validation -p pallet-mobile-registry`

## COMPAT

Regarding compatibility, most of the libraries used in all our project MUST compile
with an old Rust toolchain cf https://github.com/integritee-network/worker/blob/0a401bcaabb27ba4e5f59357b0a324c311b39ecd/rust-toolchain.toml

To this end, in `./compat` folder there is a `Cargo.lock` and a bunch of symlinks
that are there to make sure we test against this old toolchain in CI.
