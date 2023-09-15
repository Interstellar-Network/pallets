/// https://github.com/paritytech/substrate/blob/monthly-2022-10/frame/examples/offchain-worker/src/tests.rs
use frame_support::pallet_prelude::DispatchError;
use frame_support::pallet_prelude::Hooks;
use frame_support::{assert_err, assert_ok};
use sp_core::Encode;
use sp_runtime::ModuleError;

use crate::mock::*;
use crate::GrpcCallKind;
use crate::IndexingData;

type BlockNumber = frame_system::pallet_prelude::BlockNumberFor<Test>;

fn test_pub() -> sp_core::sr25519::Public {
    sp_core::sr25519::Public::from_raw([1u8; 32])
}

fn prepare_ocw_storage(call_kind: GrpcCallKind) {
    // TODO? https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/frame/merkle-mountain-range/src/tests.rs#L38C1-L46C2
    //      fn new_block() {
    // TODO? https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/client/cli/src/params/offchain_worker_params.rs#L48
    // TODO? https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/frame/im-online/src/mock.rs etc

    // https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/frame/im-online/src/tests.rs#L205
    // cf /.../.cargo/git/checkouts/substrate-7e08433d4c370a21/8c4b845/primitives/runtime/src/offchain/storage.rs
    // "given"
    // let block = 1;
    // System::set_block_number(block);

    // cf Substrate "fn test_offchain_local_storage"
    // TODO Calling the extrinsinc SHOULD set the local storate but apparently not
    // It COULD be related to "enable-offchain-indexing"
    let storage_kind = sp_core::offchain::StorageKind::PERSISTENT;
    let key = PalletOcwCircuits::derived_key();
    assert_eq!(sp_io::offchain::local_storage_get(storage_kind, &key), None);
    let data = IndexingData {
        grpc_kind: call_kind,
    };
    sp_io::offchain::local_storage_set(storage_kind, &key, &data.encode());
}

#[cfg(feature = "circuit-gen-rs")]
async fn test_submit_config_generic_signed(
    mock_type: MockType,
) -> (
    Result<(), sp_runtime::DispatchError>,
    sp_io::TestExternalities,
) {
    let (mut t,) = new_test_ext(mock_type, true).await;
    let res = t.execute_with(|| {
        let account_id = test_pub();

        // Dispatch a signed extrinsic.
        let res = PalletOcwCircuits::submit_config_generic_signed(
            RuntimeOrigin::signed(account_id),
            "PLACEHOLDER_HASH".into(),
        );
        // TODO how to CHECK "append_or_replace_verilog_hash"
        // System::assert_last_event(crate::Event::NewMobileRegistered { account_id: 1 }.into());

        prepare_ocw_storage(GrpcCallKind::Generic {
            verilog_cid: "PLACEHOLDER_HASH".into(),
        });

        res
    });

    (res, t)
}

#[cfg(feature = "circuit-gen-rs")]
async fn test_submit_config_display_circuits_package_signed(
    mock_type: MockType,
) -> (
    Result<(), sp_runtime::DispatchError>,
    sp_io::TestExternalities,
) {
    let (mut t,) = new_test_ext(mock_type, false).await;
    let res = t.execute_with(|| {
        let account_id = test_pub();

        // Dispatch a signed extrinsic.
        let res = PalletOcwCircuits::submit_config_display_circuits_package_signed(
            RuntimeOrigin::signed(account_id),
        );
        // TODO how to CHECK "append_or_replace_verilog_hash"
        // System::assert_last_event(crate::Event::NewMobileRegistered { account_id: 1 }.into());

        prepare_ocw_storage(GrpcCallKind::Display);

        res
    });

    (res, t)
}

/// If IPFS is down; it MUST NOT panic/crash/etc
#[cfg(feature = "circuit-gen-rs")]
#[tokio::test]
#[serial_test::serial]
async fn test_submit_config_generic_signed_ipfs_down_does_not_panic() {
    let (res, mut t) = test_submit_config_generic_signed(MockType::IpfsDown).await;

    // the core logic is in "offchain_worker"; so the extrinsic itself SHOULD be OK
    assert_ok!(res);

    t.execute_with(|| {
        PalletOcwCircuits::offchain_worker(1);
    });

    // TODO check error in logs? other way?
}

/// If IPFS is down; it MUST NOT panic/crash/etc
#[cfg(feature = "circuit-gen-rs")]
#[tokio::test]
#[serial_test::serial]
async fn test_submit_config_display_circuits_package_signed_ipfs_down_does_not_panic() {
    let (res, mut t) = test_submit_config_display_circuits_package_signed(MockType::IpfsDown).await;

    // the core logic is in "offchain_worker"; so the extrinsic itself SHOULD be OK
    assert_ok!(res);

    t.execute_with(|| {
        PalletOcwCircuits::offchain_worker(1);
    });

    // TODO check error in logs? other way?
}

/// [generic] If the given IPFS hash is NOT a valid Verilog file; it should fail gracefully
#[cfg(feature = "circuit-gen-rs")]
#[tokio::test]
#[serial_test::serial]
async fn test_submit_config_generic_signed_not_a_verilog_file_does_not_panic() {
    let (res, mut t) = test_submit_config_generic_signed(MockType::InvalidVerilog).await;

    // the core logic is in "offchain_worker"; so the extrinsic itself SHOULD be OK
    assert_ok!(res);

    t.execute_with(|| {
        PalletOcwCircuits::offchain_worker(1);
    });
}

#[cfg(feature = "circuit-gen-rs")]
#[tokio::test]
#[serial_test::serial]
async fn test_submit_config_generic_signed_ok() {
    let (res, mut t) = test_submit_config_generic_signed(MockType::DisplayValid).await;

    // the core logic is in "offchain_worker"; so the extrinsic itself SHOULD be OK
    assert_ok!(res);

    t.execute_with(|| {
        PalletOcwCircuits::offchain_worker(1);
    });
}

/// NOTE: this test is slow!
/// on local machine: finished in 211.52s
#[cfg(feature = "circuit-gen-rs")]
#[tokio::test]
#[serial_test::serial]
async fn test_submit_config_display_circuits_package_signed_ok() {
    let (res, mut t) =
        test_submit_config_display_circuits_package_signed(MockType::DisplayValid).await;

    // the core logic is in "offchain_worker"; so the extrinsic itself SHOULD be OK
    assert_ok!(res);

    t.execute_with(|| {
        PalletOcwCircuits::offchain_worker(1);
    });
}

/// Starting multiple offchain_worker SHOULD get "nothing to do, returning..."
// TODO this is not ideal; we should start the jobs in parallel; BUT it is better than nothing
#[cfg(feature = "circuit-gen-rs")]
#[tokio::test]
#[serial_test::serial]
async fn test_only_one_job_can_be_running() {
    let (res, mut t) = test_submit_config_generic_signed(MockType::DisplayValid).await;

    // the core logic is in "offchain_worker"; so the extrinsic itself SHOULD be OK
    assert_ok!(res);

    t.execute_with(|| {
        PalletOcwCircuits::offchain_worker(1);
        PalletOcwCircuits::offchain_worker(2);
    });
}
