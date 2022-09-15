use crate::{mock::*, Error};
use frame_support::assert_ok;
use frame_support::pallet_prelude::ConstU32;
use frame_support::{assert_err, BoundedVec};

#[test]
fn store_metadata_ok() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.
        let account_id = 1;
        let ipfs_cid = vec![1, 2];
        assert_ok!(TxRegistry::store_tx_result(
            Origin::signed(account_id),
            ipfs_cid.clone(),
            result,
        ));
        // Read pallet storage and assert an expected result.
        // MUST match the value [1,2] given to store_metadata
        let key_ipfs_hash: BoundedVec<u8, ConstU32<64>> = ipfs_cid.clone().try_into().unwrap();
        // MUST match the value [3,4] given to store_metadata
        let expected_message_digits: BoundedVec<u8, ConstU32<10>> =
            message_digits.clone().try_into().unwrap();
        let expected_pinpad_digits: BoundedVec<u8, ConstU32<10>> =
            pinpad_digits.clone().try_into().unwrap();
        let stored = TxRegistry::circuit_server_metadata_map(account_id, key_ipfs_hash).unwrap();
        assert_eq!(stored.message_digits, expected_message_digits);
        assert_eq!(stored.pinpad_digits, expected_pinpad_digits);
    });
}
