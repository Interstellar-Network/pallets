# pallet tx-registry

This is a `node+worker` pallet.
It DOES NOT REQUIRE `fn offchain_worker`.

It is DEMO purposes to be able to display the result of `pallet_tx_validation::check_input`(called by the worker using DirectCall)
on the front-end.
