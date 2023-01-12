# pallets

Those are the pallets meant to be used either through(ie as a submodule):
- https://github.com/Interstellar-Network/substrate-offchain-worker-demo
- https://github.com/Interstellar-Network/integritee-worker

It is NOT meant to work as standalone.

## WIP standalone tests

NOTE: those are using Substrate testing framework, not Integritee(if it even exists).

`[RUST_BACKTRACE=1] cargo test [--no-fail-fast] -p pallet-ocw-garble -p pallet-ocw-circuits -p pallet-tx-validation -p pallet-mobile-registry`