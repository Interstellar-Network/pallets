use crate as pallet_ocw_garble;
use crate::*;
use frame_support::{
    parameter_types,
    traits::{ConstU32, ConstU64},
};
use httpmock::prelude::*;
use serde_json::json;
use sp_core::{
    offchain::{testing, OffchainWorkerExt},
    sr25519::Signature,
    H256,
};
use sp_runtime::{
    testing::{Header, TestXt},
    traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify},
};
use std::io::Cursor;
use tests_utils::foreign_ipfs;
use tests_utils::foreign_ipfs::IpfsApi;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
    pub enum Test where
        Block = Block,
        NodeBlock = Block,
        UncheckedExtrinsic = UncheckedExtrinsic,
    {
        System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
        TxValidation: pallet_tx_validation,
        PalletOcwGarble: pallet_ocw_garble,
    }
);

impl pallet_tx_validation::Config for Test {
    type RuntimeEvent = RuntimeEvent;
}

impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = sp_core::sr25519::Public;
    // type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

type Extrinsic = TestXt<RuntimeCall, ()>;
type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

impl frame_system::offchain::SigningTypes for Test {
    type Public = <Signature as Verify>::Signer;
    type Signature = Signature;
}

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Test
where
    RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Test
where
    RuntimeCall: From<LocalCall>,
{
    fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
        call: RuntimeCall,
        _public: <Signature as Verify>::Signer,
        _account: AccountId,
        nonce: u64,
    ) -> Option<(RuntimeCall, <Extrinsic as ExtrinsicT>::SignaturePayload)> {
        Some((call, (nonce, ())))
    }
}

parameter_types! {
    pub const UnsignedPriority: u64 = 1 << 20;
}

const OVERWRITTEN_SERIALIZED_IPFS_ADD: &[u8] = &[42, 42];

pub struct MyTestCallbackMock;
impl MyTestCallback for MyTestCallbackMock {
    fn my_test_hook(_input: Vec<u8>) -> Vec<u8> {
        // MUST match mock_ipfs_add_response
        OVERWRITTEN_SERIALIZED_IPFS_ADD.to_vec()
    }
}

impl pallet_ocw_garble::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type AuthorityId = crypto::TestAuthId;
    type HookCallGrpGarbleAndStripSerializedPackageForEval = MyTestCallbackMock;
}

pub(crate) enum MockType {
    /// standard use case; two valid .skcd already present in IPFS via pallet-ocw-circuits
    RpcOcwCircuitsStorageValid,
    /// error case: the IPFS hash point to something that is NOT a .skcd
    /// it SHOULD fail at "lib_garble_rs::garble_skcd"
    InvalidSkcd,
    /// error case: CAN NOT connect to the node
    RpcOcwCircuitsStorageNoResponse,
    /// error case: the node contains ocwCircuits; BUT they are not valid IPFS hash
    RpcOcwCircuitsStorageInvalidHashes,
    /// error case: can not connect to IPFS,
    IpfsDown,
}

/// Build genesis storage according to the mock runtime.
///
/// should_mock_rpc_ocw_circuits_storage_valid:
/// should_mock_rpc_ocw_circuits_storage_bad_hashes: pallet-ocw-circuits, but the IPFS hashes point to nowhere
pub(crate) async fn new_test_ext(
    mock_type: MockType,
) -> (sp_io::TestExternalities, foreign_ipfs::ForeignNode) {
    // MOCK "integritee-node" RPC
    let mock_server_uri_node = MockServer::start();
    std::env::set_var("INTERSTELLAR_URI_NODE", mock_server_uri_node.base_url());

    let (offchain, state) = testing::TestOffchainExt::new();
    let mut t = sp_io::TestExternalities::default();
    t.register_extension(OffchainWorkerExt::new(offchain));

    // NOTE: PORT hardcoded in lib.rs so we can use a dynamic one
    let (foreign_node, ipfs_reference_client) = foreign_ipfs::run_ipfs_in_background(None);
    std::env::set_var(
        "IPFS_ROOT_URL",
        format!("http://127.0.0.1:{}", foreign_node.api_port),
    );

    match mock_type {
        MockType::RpcOcwCircuitsStorageValid | MockType::IpfsDown | MockType::InvalidSkcd => {
            // IPFS ADD the .skcd needed
            // let content = &[65u8, 90, 97, 122]; // AZaz
            let cursor = match mock_type {
                MockType::InvalidSkcd => Cursor::new(vec![42, 42]),
                _ => {
                    let bytes =
                        include_bytes!("../tests/data/display_message_120x52_2digits.skcd.pb.bin")
                            .to_vec();
                    Cursor::new(bytes)
                }
            };
            let ipfs_add_response_1 = ipfs_reference_client.add(cursor).await.unwrap();
            let cursor = Cursor::new(include_bytes!(
                "../tests/data/display_pinpad_590x50.skcd.pb.bin"
            ));
            let ipfs_add_response_2 = ipfs_reference_client.add(cursor).await.unwrap();

            mock_ocw_circuits_storage_value_response(
                &mock_server_uri_node,
                ipfs_add_response_1.hash,
                ipfs_add_response_2.hash,
            );

            match mock_type {
                MockType::IpfsDown => {
                    // "Kill the server": use a bad env var "IPFS_ROOT_URL"
                    std::env::set_var("IPFS_ROOT_URL", format!("http://127.0.0.1:{}", "4242"));
                }
                _ => {}
            }
        }
        MockType::RpcOcwCircuitsStorageNoResponse => {
            // nothing to do
        }
        MockType::RpcOcwCircuitsStorageInvalidHashes => {
            mock_ocw_circuits_storage_value_response(
                &mock_server_uri_node,
                // anything should work
                // obtained by grep "ipfs cat /ipfs/" on https://docs.ipfs.tech/how-to/command-line-quick-start/#initialize-the-repository
                "NOT_A_HASH".to_string(),
                "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG".to_string(),
            );
        }
    }

    (t, foreign_node)
}

/// Compute the Storage key
/// cf https://docs.substrate.io/build/remote-procedure-calls/
/// NOTE: this is bad, it will fail if the storage changes, and the compiler will not catch it!
/// https://substrate.stackexchange.com/questions/3354/access-storage-map-from-another-pallet-without-trait-pallet-config
fn compute_storage_hash_hex(pallet: &str, storage_key: &str) -> String {
    let pallet_hash = sp_io::hashing::twox_128(pallet.as_bytes());
    let storage_hash = sp_io::hashing::twox_128(storage_key.as_bytes());

    let mut final_key = Vec::new();
    final_key.extend_from_slice(&pallet_hash);
    final_key.extend_from_slice(&storage_hash);

    "0x".to_string() + &hex::encode(final_key)
}

/// For now b/c https://github.com/integritee-network/worker/issues/976
/// we query the storage using a HTTP request; so we need to Mock it
/// cf "fn get_ocw_circuits_storage_value"
///
/// based on https://github.com/paritytech/substrate/blob/e9b0facf70eeb08032cc7e83548c62f0b4a24bb1/frame/examples/offchain-worker/src/tests.rs#L385
fn mock_ocw_circuits_storage_value_response(
    server: &MockServer,
    message_skcd_cid: String,
    pinpad_skcd_cid: String,
) {
    let body_json = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method":"state_getStorage",
        "params": [compute_storage_hash_hex("OcwCircuits", "DisplaySkcdPackageValue")]
    });
    let body = serde_json::to_string(&body_json).unwrap();

    let display_skcd_package = DisplaySkcdPackageCopy {
        message_skcd_cid: message_skcd_cid.as_bytes().to_vec(),
        message_skcd_server_metadata_nb_digits: 2,
        pinpad_skcd_cid: pinpad_skcd_cid.as_bytes().to_vec(),
        pinpad_skcd_server_metadata_nb_digits: 10,
    };
    let display_skcd_package_encoded = display_skcd_package.encode();
    let display_skcd_package_encoded_hex =
        "0x".to_string() + &hex::encode(display_skcd_package_encoded);

    let response_body_json = json!({
        "id": "1",
        "jsonrpc": "2.0",
        "result": display_skcd_package_encoded_hex,
    });
    let response_body = serde_json::to_vec(&response_body_json).unwrap();

    server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .header("Content-Type", "application/json;charset=utf-8")
            .body(&body);
        then.status(200)
            .header("content-type", "application/json; charset=utf-8")
            // cf "fn decode_rpc_json" for the expected format
            // MUST match the param passed to "mock_ipfs_cat_response"
            .body(response_body);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_storage_hash_hex_reference() {
        assert_eq!(
            compute_storage_hash_hex("OcwCircuits", "DisplaySkcdPackageValue"),
            "0x2c644167ae9423d1f0683de9002940b8bd009489ffa75ba4c0b3f4f6fed7414b"
        );
    }
}
