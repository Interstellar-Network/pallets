use crate::mock::*;
use frame_support::assert_ok;
use test_log::test;

#[test]
fn store_tx_result_txpass_ok() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.
        let account_id = 1;
        let ipfs_cid = vec![1, 2];
        assert_ok!(TxRegistry::store_tx_result(
            RuntimeOrigin::signed(account_id),
            ipfs_cid.clone(),
            crate::TxResult::TxPass,
        ));

        let stored = TxRegistry::tx_results_map(account_id).unwrap();
        let first = stored.first().unwrap();

        assert_eq!(first.message_pgarbled_cid, ipfs_cid);
        assert_eq!(first.result, crate::TxResult::TxPass,);
    });
}

#[test]
fn store_tx_result_txfail_ok() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.
        let account_id = 1;
        let ipfs_cid = vec![1, 2];
        assert_ok!(TxRegistry::store_tx_result(
            RuntimeOrigin::signed(account_id),
            ipfs_cid.clone(),
            crate::TxResult::TxFail,
        ));

        let stored = TxRegistry::tx_results_map(account_id).unwrap();
        let first = stored.first().unwrap();

        assert_eq!(first.message_pgarbled_cid, ipfs_cid);
        assert_eq!(first.result, crate::TxResult::TxFail,);
    });
}

/// Test that pushing 2 tx results works fine
#[test]
fn store_tx_result_multiple_ok() {
    new_test_ext().execute_with(|| {
        // Dispatch a signed extrinsic.
        let account_id = 1;
        let ipfs_cid_1 = vec![1, 2];
        assert_ok!(TxRegistry::store_tx_result(
            RuntimeOrigin::signed(account_id),
            ipfs_cid_1.clone(),
            crate::TxResult::TxPass,
        ));

        let ipfs_cid_2 = vec![3, 4];
        assert_ok!(TxRegistry::store_tx_result(
            RuntimeOrigin::signed(account_id),
            ipfs_cid_2.clone(),
            crate::TxResult::TxFail,
        ));

        let stored = TxRegistry::tx_results_map(account_id).unwrap();
        let first = stored.get(0).unwrap();
        let second = stored.get(1).unwrap();

        assert_eq!(first.message_pgarbled_cid, ipfs_cid_1);
        assert_eq!(first.result, crate::TxResult::TxPass,);
        assert_eq!(second.message_pgarbled_cid, ipfs_cid_2);
        assert_eq!(second.result, crate::TxResult::TxFail,);
    });
}
