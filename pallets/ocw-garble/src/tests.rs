/// https://github.com/paritytech/substrate/blob/monthly-2022-10/frame/examples/offchain-worker/src/tests.rs
use crate::mock::*;
use frame_support::pallet_prelude::DispatchError;
use frame_support::{assert_err, assert_ok};
use sp_runtime::ModuleError;

fn test_pub() -> sp_core::sr25519::Public {
    sp_core::sr25519::Public::from_raw([1u8; 32])
}

async fn test_garble_and_strip_display_circuits_package_signed(
    mock_type: MockType,
) -> Result<(), sp_runtime::DispatchError> {
    let (mut t, foreign_node) = new_test_ext(mock_type).await;
    let res = t.execute_with(|| {
        let account_id = test_pub();

        // Dispatch a signed extrinsic.
        PalletOcwGarble::garble_and_strip_display_circuits_package_signed(
            RuntimeOrigin::signed(account_id),
            vec![42],
        )
        // TODO how to CHECK "append_or_replace_verilog_hash"
        // System::assert_last_event(crate::Event::NewMobileRegistered { account_id: 1 }.into());
    });

    // Needed to keep the server alive?
    assert!(foreign_node.daemon.id() > 0);

    res
}

/// If the RPC to query ocwCircuits fails; it MUST NOT panic/crash/etc
#[tokio::test]
#[serial_test::serial]
async fn test_rpc_ocw_circuits_storage_value_no_response_does_not_panic() {
    let res = test_garble_and_strip_display_circuits_package_signed(
        MockType::RpcOcwCircuitsStorageNoResponse,
    )
    .await;
    assert_err!(
        res,
        DispatchError::Module(ModuleError {
            index: 2,
            error: [5, 0, 0, 0],
            message: Some("HttpFetchingError")
        })
    );
}

/// If the RPC to query ocwCircuits return invalid IPFS hashes; it MUST NOT panic/crash/etc
#[tokio::test]
#[serial_test::serial]
async fn test_rpc_ocw_circuits_storage_value_invalid_hashes_does_not_panic() {
    let res = test_garble_and_strip_display_circuits_package_signed(
        MockType::RpcOcwCircuitsStorageInvalidHashes,
    )
    .await;
    assert_err!(
        res,
        DispatchError::Module(ModuleError {
            index: 2,
            error: [8, 0, 0, 0],
            message: Some("IpfsCallError")
        }),
    );
}

/// If IPFS is down; it MUST NOT panic/crash/etc
#[tokio::test]
#[serial_test::serial]
async fn test_rpc_ocw_circuits_ipfs_down_does_not_panic() {
    let res = test_garble_and_strip_display_circuits_package_signed(MockType::IpfsDown).await;
    assert_err!(
        res,
        DispatchError::Module(ModuleError {
            index: 2,
            error: [8, 0, 0, 0],
            message: Some("IpfsCallError")
        }),
    );
}

/// If the .skcd are not valid; it MUST NOT panic/crash/etc
#[tokio::test]
#[serial_test::serial]
async fn test_rpc_ocw_circuits_invalid_skcd_does_not_panic() {
    let res = test_garble_and_strip_display_circuits_package_signed(MockType::InvalidSkcd).await;
    assert_err!(
        res,
        DispatchError::Module(ModuleError {
            index: 2,
            error: [9, 0, 0, 0],
            message: Some("GarblerError")
        }),
    );
}

#[tokio::test]
#[serial_test::serial]
async fn test_garble_and_strip_display_circuits_package_signed_ok() {
    let res =
        test_garble_and_strip_display_circuits_package_signed(MockType::RpcOcwCircuitsStorageValid)
            .await;
    assert_ok!(res);
}
