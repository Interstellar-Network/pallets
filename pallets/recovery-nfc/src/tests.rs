use frame_support::pallet_prelude::ConstU32;
use frame_support::pallet_prelude::DispatchError;
use frame_support::{assert_err, assert_ok};
use sp_runtime::traits::Lookup;
use sp_runtime::ModuleError;
use test_log::test;

use crate::{mock::*, Config, Error};

fn setup_create_recovery(account_id: u64, should_be_ok: bool, hashed_nfc_tag: &[u8]) {
    let origin = RuntimeOrigin::signed(account_id);
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
        origin.clone(),
        friends,
        threshold,
        delay_period
    ));

    // Some time has passed, and the user lost their keys!
    run_to_block(10);

    if should_be_ok {
        assert_ok!(RecoveryNfc::create_recovery_nfc(
            origin.clone(),
            hashed_nfc_tag.to_vec(),
        ));
    }
}

/// When expecting 2 digits, giving eg 4 inputs SHOULD graciously fail
/// It MUST fail with the standard "tx fail" error(ie TxWrongInputGiven) to avoid leaking the expected input length
/// NOTE: the expected input length is indeed present on the client side, but it is NOT leaked from this pallet.
/// It comes from pallet_ocw_garble.
///
/// NOTE: this DOES NOT return an Err; that way the tx IS NOT rollbacked
/// and the user CAN NOT retry
fn test_check_input_ok(hashed_nfc_tag: Vec<u8>, should_be_ok: bool) {
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

        setup_create_recovery(account_id, true, &hashed_nfc_tag);

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
            hashed_nfc_tag
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

#[test]
fn check_recovery_life_cycle_without_existing_recovery_ok() {
    new_test_ext().execute_with(|| {
        let account_id = 1;
        let hashed_nfc_tag = vec![3, 4];
        let origin = RuntimeOrigin::signed(account_id);
        assert_ok!(RecoveryNfc::create_recovery_nfc(
            origin.clone(),
            hashed_nfc_tag.clone(),
        ));

        // PREREQ pallet_recovery::initiate_recovery
        Recovery::initiate_recovery(origin.clone(), account_id).unwrap();

        // Dispatch a signed extrinsic.
        // Ensure the expected error is thrown if a wrong input is given
        let result = RecoveryNfc::vouch_with_nfc_tag(origin.clone(), hashed_nfc_tag);
        assert_ok!(result);
        // TODO? should this be a noop?
        // assert_noop!(
        //     RecoveryNfc::check_input(Origin::signed(account_id), ipfs_cid.clone(), vec![0, 0]),
        //     Error::<Test>::TxWrongInputGiven
        // );

        System::assert_has_event(
            pallet_recovery::Event::RecoveryVouched {
                lost_account: account_id,
                rescuer_account: account_id,
                sender: account_id,
            }
            .into(),
        );
        System::assert_last_event(
            crate::Event::VouchedWithNfc {
                account_id: account_id,
            }
            .into(),
        );
    });
}

#[test]
fn check_recovery_life_cycle_with_existing_recovery_ok() {
    new_test_ext().execute_with(|| {
        let account_id = 1;
        let origin = RuntimeOrigin::signed(account_id);
        let hashed_nfc_tag = vec![3, 4];

        setup_create_recovery(account_id, true, &hashed_nfc_tag);

        // PREREQ pallet_recovery::initiate_recovery
        Recovery::initiate_recovery(origin.clone(), account_id).unwrap();

        // Dispatch a signed extrinsic.
        // Ensure the expected error is thrown if a wrong input is given
        let result = RecoveryNfc::vouch_with_nfc_tag(origin.clone(), hashed_nfc_tag);
        assert_ok!(result);
        // TODO? should this be a noop?
        // assert_noop!(
        //     RecoveryNfc::check_input(Origin::signed(account_id), ipfs_cid.clone(), vec![0, 0]),
        //     Error::<Test>::TxWrongInputGiven
        // );
    });
}

#[test]
fn check_recovery_life_cycle_vouch_wrong_tag_should_fail() {
    new_test_ext().execute_with(|| {
        let account_id = 1;
        let origin = RuntimeOrigin::signed(account_id);
        let hashed_nfc_tag_setup = vec![3, 4];
        let hashed_nfc_tag_vouch = vec![5, 6];

        setup_create_recovery(account_id, true, &hashed_nfc_tag_setup);

        // PREREQ pallet_recovery::initiate_recovery
        Recovery::initiate_recovery(origin.clone(), account_id).unwrap();

        // Dispatch a signed extrinsic.
        // Ensure the expected error is thrown if a wrong input is given
        let result = RecoveryNfc::vouch_with_nfc_tag(origin.clone(), hashed_nfc_tag_vouch);
        // assert_err!(
        //     result,
        //     DispatchError::Module(ModuleError {
        //         index: 3,
        //         error: [1, 0, 0, 0],
        //         message: Some("InvalidNfcTag")
        //     })
        // );
        assert_ok!(result);
        System::assert_last_event(
            crate::Event::InvalidNfcSn {
                account_id: account_id,
            }
            .into(),
        );
    });
}
