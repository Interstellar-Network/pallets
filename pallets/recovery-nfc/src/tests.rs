use frame_support::pallet_prelude::ConstU32;
use frame_support::pallet_prelude::DispatchError;
use frame_support::{assert_err, assert_ok};
use sp_runtime::traits::Lookup;
use sp_runtime::DispatchResult;
use sp_runtime::ModuleError;
use test_log::test;

use crate::{mock::*, Config, Error};

/// (Optionnaly) call to `pallet_recovery::create_recovery`
fn setup_create_recovery(account_id: u64) {
    let origin = RuntimeOrigin::signed(account_id);
    // cf pallet-recovery's `recovery_life_cycle_works`
    let friends = vec![account_id];
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
}

/// When expecting 2 digits, giving eg 4 inputs SHOULD graciously fail
/// It MUST fail with the standard "tx fail" error(ie TxWrongInputGiven) to avoid leaking the expected input length
/// NOTE: the expected input length is indeed present on the client side, but it is NOT leaked from this pallet.
/// It comes from pallet_ocw_garble.
///
/// NOTE: this DOES NOT return an Err; that way the tx IS NOT rollbacked
/// and the user CAN NOT retry
fn run_vouch_with_nfc_tag(
    account_id_setup: u64,
    account_id_initiate: u64,
    account_id_vouch: u64,
    // For now we make sure it works both when `create_recovery` have been called, and if not
    should_call_create_recovery: bool,
    should_call_create_recovery_nfc: bool,
    // Contrary to `create_recovery`, calling `pallet_recovery::initiate_recovery` is mandatory
    should_call_initiate_recovery: bool,
    nfc_tag_setup: Vec<u8>,
    nfc_tag_vouch: Vec<u8>,
    finalize: fn(vouch_with_nfc_tag_result: DispatchResult),
) {
    new_test_ext().execute_with(|| {
        // let ipfs_cid = vec![1, 2];
        // assert_ok!(RecoveryNfc::create_recovery_nfc(
        //     RuntimeOrigin::signed(account_id),
        //     ipfs_cid.clone(),
        //     // store_metadata is raw, as-is(no ascii conv)
        //     vec![3, 4],
        //     vec![4, 5, 6, 0, 1, 2, 3, 7, 8, 9],
        // ));

        if should_call_create_recovery {
            setup_create_recovery(account_id_setup);
        }

        if should_call_create_recovery_nfc {
            assert_ok!(RecoveryNfc::create_recovery_nfc(
                RuntimeOrigin::signed(account_id_setup),
                nfc_tag_setup,
            ));
        }

        // PREREQ pallet_recovery::initiate_recovery
        // "Using account 1, the user begins the recovery process to recover the lost account"
        if should_call_initiate_recovery {
            assert_ok!(Recovery::initiate_recovery(
                RuntimeOrigin::signed(account_id_initiate),
                account_id_setup
            ));
        }
        // "Off chain, the user contacts their friends and asks them to vouch for the recovery
        // attempt"

        // Dispatch a signed extrinsic.
        let res =
            RecoveryNfc::vouch_with_nfc_tag(RuntimeOrigin::signed(account_id_vouch), nfc_tag_vouch);

        finalize(res);
    });
}

/// Not really a standard case: we test if calling `vouch_with_nfc_tag` using
/// the same account than `create_recovery_nfc` is OK
#[test]
fn test_vouch_with_nfc_tag_recovery_with_same_account_ok() {
    run_vouch_with_nfc_tag(1, 1, 1, true, true, true, vec![42], vec![42], |_res| {});
}

/// Not really a standard case: we test if calling `vouch_with_nfc_tag` using
/// the same account than `create_recovery_nfc` is OK
#[test]
fn test_vouch_with_nfc_tag_without_existing_recovery_ok() {
    run_vouch_with_nfc_tag(1, 1, 1, false, true, true, vec![42], vec![42], |res| {
        assert_ok!(res);
        System::assert_has_event(
            pallet_recovery::Event::RecoveryVouched {
                lost_account: 1,
                rescuer_account: 1,
                sender: 1,
            }
            .into(),
        );
        System::assert_last_event(crate::Event::VouchedWithNfc { account_id: 1 }.into());
    });
}

/// The standard use case:
/// - Account1 is used for setup
/// - Account2 is used for recovery
#[test]
fn test_vouch_with_nfc_tag_standard_lifecycle_ok() {
    // NOTE: we vouch with Account2, but due to how we handle `pallet_recovery`, the event is only Account1 for all fields
    run_vouch_with_nfc_tag(1, 1, 2, false, true, true, vec![42], vec![42], |res| {
        assert_ok!(res);
        System::assert_has_event(
            pallet_recovery::Event::RecoveryVouched {
                lost_account: 1,
                rescuer_account: 1,
                sender: 1,
            }
            .into(),
        );
        System::assert_last_event(crate::Event::VouchedWithNfc { account_id: 2 }.into());
    });
}

/// The standard use case:
/// - Account1 is used for setup
/// - Account2 is used for recovery
///
/// But using the wrong NFC S/N
#[test]
fn test_vouch_with_nfc_tag_wrong_tag_should_fail() {
    run_vouch_with_nfc_tag(1, 1, 2, false, true, true, vec![41], vec![42], |res| {
        assert_ok!(res);
        System::assert_last_event(crate::Event::UnknownAccount { account_id: 2 }.into());
    });
}
