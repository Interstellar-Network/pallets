# pallet tx-validation

This is a `worker` pallet.
It DOES NOT REQUIRE `fn offchain_worker`.

IT contains what is needed to check the user-given app inputs(ie digits entered on the pinpad) against
the expected code+pinpad permutations.
