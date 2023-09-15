# pallet ocw-circuits

This is a `node` pallet.
It REQUIRES `fn offchain_worker`.

Its main purpose is to handle the generation of the "master circuits(skcd)".

## features

Because this pallet is used both:
- directly by the node to generate new circuits from the front-end
- and from `pallet-ocw-garble` to access the "skcd" generated

There is a "circuit-gen-rs" feature controlling if the dependency "circuit-gen-rs" is pulled or not.