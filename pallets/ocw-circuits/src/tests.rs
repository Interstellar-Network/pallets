/// https://github.com/paritytech/substrate/blob/monthly-2022-10/frame/examples/offchain-worker/src/tests.rs
use crate::mock::*;
use frame_support::assert_ok;
use test_log::test;

fn test_pub() -> sp_core::sr25519::Public {
    sp_core::sr25519::Public::from_raw([1u8; 32])
}

fn test_submit_config_display_circuits_package_signed() {
    new_test_ext().execute_with(|| {
        let account_id = test_pub();

        // Dispatch a signed extrinsic.
        assert_ok!(
            PalletOcwCircuits::submit_config_display_circuits_package_signed(
                RuntimeOrigin::signed(account_id),
            )
        );
        // TODO how to CHECK "append_or_replace_verilog_hash"
        // System::assert_last_event(crate::Event::NewMobileRegistered { account_id: 1 }.into());
    });
}

#[test]
fn test_submit_config_display_circuits_package_signed_ok() {
    test_submit_config_display_circuits_package_signed()
}
