use crate::{mock::*, Error};
use frame_support::assert_ok;
use frame_support::pallet_prelude::ConstU32;
use frame_support::{assert_err, BoundedVec};
use test_log::test;

#[test]
fn store_metadata_ok() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.
        let account_id = 1;
        let ipfs_cid = vec![1, 2];
        let message_digits = vec![3, 4];
        let pinpad_digits = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_ok!(RecoveryNfc::create_recovery_nfc(
            RuntimeOrigin::signed(account_id),
            ipfs_cid.clone(),
            message_digits.clone(),
            pinpad_digits.clone(),
        ));
        // Read pallet storage and assert an expected result.
        // MUST match the value [1,2] given to store_metadata
        let key_ipfs_hash: BoundedVec<u8, ConstU32<64>> = ipfs_cid.clone().try_into().unwrap();
        // MUST match the value [3,4] given to store_metadata
        let expected_message_digits: BoundedVec<u8, ConstU32<10>> =
            message_digits.clone().try_into().unwrap();
        let expected_pinpad_digits: BoundedVec<u8, ConstU32<10>> =
            pinpad_digits.clone().try_into().unwrap();
        let stored = RecoveryNfc::circuit_server_metadata_map(account_id, key_ipfs_hash).unwrap();
        assert_eq!(stored.message_digits, expected_message_digits);
        assert_eq!(stored.pinpad_digits, expected_pinpad_digits);
    });
}

/// When expecting 2 digits, giving eg 4 inputs SHOULD graciously fail
/// It MUST fail with the standard "tx fail" error(ie TxWrongInputGiven) to avoid leaking the expected input length
/// NOTE: the expected input length is indeed present on the client side, but it is NOT leaked from this pallet.
/// It comes from pallet_ocw_garble.
///
/// NOTE: this DOES NOT return an Err; that way the tx IS NOT rollbacked
/// and the user CAN NOT retry
fn test_check_input_ok(inputs: Vec<u8>, should_be_ok: bool) {
    new_test_ext().execute_with(|| {
        // TODO(recovery)?
        let account_id = 1;
        // let ipfs_cid = vec![1, 2];
        // assert_ok!(RecoveryNfc::create_recovery_nfc(
        //     RuntimeOrigin::signed(account_id),
        //     ipfs_cid.clone(),
        //     // store_metadata is raw, as-is(no ascii conv)
        //     vec![3, 4],
        //     vec![4, 5, 6, 0, 1, 2, 3, 7, 8, 9],
        // ));

        // cf pallet-recovery's `recovery_life_cycle_works`
        let friends = if should_be_ok {
            vec![account_id]
        } else {
            vec![42]
        };
        let threshold = 1;
        let delay_period = 10;
        // Account 5 sets up a recovery configuration on their account
        assert_ok!(Recovery::create_recovery(
            RuntimeOrigin::signed(account_id),
            friends,
            threshold,
            delay_period
        ));
        // Some time has passed, and the user lost their keys!
        run_to_block(10);
        // Using account 1, the user begins the recovery process to recover the lost account
        assert_ok!(Recovery::initiate_recovery(
            RuntimeOrigin::signed(account_id),
            account_id
        ));
        // Off chain, the user contacts their friends and asks them to vouch for the recovery
        // attempt

        // Dispatch a signed extrinsic.
        assert_ok!(RecoveryNfc::vouch_with_nfc_tag(
            RuntimeOrigin::signed(account_id),
            inputs
        ));

        // TODO(recovery)?
        // if should_be_ok {
        //     System::assert_last_event(crate::Event::TxPass { account_id }.into());
        // } else {
        //     System::assert_last_event(crate::Event::TxFail { account_id }.into());
        //     // TODO in this case we SHOULD not allow the user to retry; ie cleanup Storage etc
        // }
    });
}

/// check_input SHOULD work with ASCII(useful for testing with a front-end)
#[test]
fn check_input_good_ascii_ok() {
    test_check_input_ok(vec!['6' as u8, '0' as u8], true)
}

#[test]
fn check_input_good_u8_ok() {
    test_check_input_ok(vec![6, 0], true)
}

#[test]
fn check_input_wrong_code_fail() {
    test_check_input_ok(vec!['0' as u8, '0' as u8], false)
}

#[test]
fn check_input_wrong_size_fail() {
    test_check_input_ok(vec![0, 0, 0, 0], false)
}

/// Giving inputs outside '0'-'9' or 0-9 SHOULD fail graciously(ie no panic!)
/// in this case contrary to "check_input_wrong_size_fail_graciously" we return an Err
/// which rollbacks the transaction. This allows the user to retry.
#[test]
fn check_input_invalid_fail_err() {
    new_test_ext().execute_with(|| {
        let account_id = 1;
        let ipfs_cid = vec![1, 2];
        assert_ok!(RecoveryNfc::create_recovery_nfc(
            RuntimeOrigin::signed(account_id),
            ipfs_cid.clone(),
            // store_metadata is raw, as-is(no ascii conv)
            vec![3, 4],
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        ));

        // Dispatch a signed extrinsic.
        // Ensure the expected error is thrown if a wrong input is given
        let result =
            RecoveryNfc::vouch_with_nfc_tag(RuntimeOrigin::signed(account_id), ipfs_cid.clone());
        assert_err!(result, Error::<Test>::TxInvalidInputsGiven);
        // TODO? should this be a noop?
        // assert_noop!(
        //     RecoveryNfc::check_input(Origin::signed(account_id), ipfs_cid.clone(), vec![0, 0]),
        //     Error::<Test>::TxWrongInputGiven
        // );
    });
}
