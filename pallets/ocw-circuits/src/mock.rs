use crate as pallet_ocw_circuits;
use crate::*;
use frame_support::{
    parameter_types,
    traits::{ConstU32, ConstU64},
};
use scale_info::prelude::sync::Arc;
use sp_core::{
    offchain::{testing, OffchainDbExt, OffchainWorkerExt, TransactionPoolExt},
    sr25519::Signature,
    H256,
};
use sp_keystore::testing::KeyStore;
use sp_keystore::KeystoreExt;
use sp_keystore::SyncCryptoStore;
use sp_runtime::{
    testing::{Header, TestXt},
    traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify},
};
use std::io::Cursor;

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
        PalletOcwCircuits: pallet_ocw_circuits,
    }
);

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

const OVERWRITTEN_SERIALIZED_IPFS_ADD: &[u8] = &[40, 41, 42, 43, 44];

pub struct MyTestCallbackMock;
impl MyTestCallback for MyTestCallbackMock {
    fn my_test_hook(_input: Vec<u8>) -> Vec<u8> {
        // MUST match mock_ipfs_add_response
        OVERWRITTEN_SERIALIZED_IPFS_ADD.to_vec()
    }
}

impl pallet_ocw_circuits::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type AuthorityId = crypto::TestAuthId;
    type HookCallPostSerializedPackage = MyTestCallbackMock;
}

pub(crate) enum MockType {
    /// standard use case; everything is OK
    DisplayValid,
    /// error case: [generic circuit] the IPFS hash point to something that is NOT a valid Verilog file
    InvalidVerilog,
    /// error case: can not connect to IPFS,
    IpfsDown,
}

// Build genesis storage according to the mock runtime.
pub(crate) async fn new_test_ext(
    mock_type: MockType,
    is_generic: bool,
) -> (sp_io::TestExternalities,) {
    let _ = env_logger::try_init();

    // // system::GenesisConfig::default()
    // //     .build_storage::<Test>()
    // //     .unwrap()
    // //     .into()

    // let t = frame_system::GenesisConfig::default()
    //     .build_storage::<Test>()
    //     .unwrap();
    // // pallet_mobile_registry::GenesisConfig::<Test> {
    // //     balances: vec![(1, 10), (2, 10), (3, 10), (4, 10), (5, 2)],
    // // }
    // // .assimilate_storage(&mut t)
    // // .unwrap();

    // let mut ext = sp_io::TestExternalities::new(t);
    // ext.execute_with(|| System::set_block_number(1));
    // ext

    let t = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();
    let mut t: sp_io::TestExternalities = t.into();

    let offchain_db = testing::TestPersistentOffchainDB::new();
    let (offchain, state) = testing::TestOffchainExt::with_offchain_db(offchain_db);
    let (pool, _state) = testing::TestTransactionPoolExt::new();
    // https://github.com/JoshOrndorff/recipes/blob/03b7a0657727705faa5f840c73bcf15ffdd81f2b/pallets/ocw-demo/src/tests.rs#L112C3-L113C77
    const PHRASE: &str = "expire stage crawl shell boss any story swamp skull yellow bamboo copy";
    let keystore = KeyStore::new();
    keystore
        .sr25519_generate_new(KEY_TYPE, Some(&format!("{}/hunter1", PHRASE)))
        .unwrap();

    t.register_extension(OffchainDbExt::new(offchain.clone()));
    t.register_extension(OffchainWorkerExt::new(offchain));
    t.register_extension(TransactionPoolExt::new(pool));
    t.register_extension(KeystoreExt(Arc::new(keystore)));

    const MOCK_IPFS_ROOT_URL: &str = "http://127.0.0.1:4242";
    std::env::set_var("IPFS_ROOT_URL", MOCK_IPFS_ROOT_URL);

    // IPFS ADD the Verilog needed
    match mock_type {
        MockType::DisplayValid => {
            if is_generic {
                // Ideally we would not need to mock a request to test a server down
                // but cf "/primitives/core/src/offchain/testing.rs:304:21" it is a "panic!"
                state.write().expect_request(testing::PendingRequest {
                    method: "POST".into(),
                    uri: format!("{MOCK_IPFS_ROOT_URL}/api/v0/cat?arg={}", "PLACEHOLDER_HASH")
                        .to_string(),
                    headers: vec![],
                    body: vec![],
                    response: Some(include_bytes!("../tests/data/adder.v").to_vec()),
                    response_headers: vec![("content-type".into(), "text/plain".into())],
                    sent: true,
                    ..Default::default()
                });
            }

            // NOTE: if display: 2 requests: one for pinpad and one for message
            let nb_requests = if (is_generic) { 1 } else { 2 };
            for _ in 0..nb_requests {
                state.write().expect_request(testing::PendingRequest {
                    method: "POST".into(),
                    uri: format!("{MOCK_IPFS_ROOT_URL}/api/v0/add").to_string(),
                    headers: vec![(
                        "Content-Type".into(),
                        "multipart/form-data;boundary=\"boundary\"".into(),
                    )],
                    body: interstellar_http_client::new_multipart_body_bytes(
                        OVERWRITTEN_SERIALIZED_IPFS_ADD,
                    ),
                    response: Some(
                        r#"{
                        "Bytes": "142",
                        "Hash": "PLACEHOLDER_HASH",
                        "Name": "PLACEHOLDER_NAME",
                        "Size": "842"
                      }"#
                        .into(),
                    ),
                    response_headers: vec![(
                        "content-type".into(),
                        "application/json; charset=utf-8".into(),
                    )],
                    sent: true,
                    ..Default::default()
                });
            }
        }
        MockType::InvalidVerilog => {
            state.write().expect_request(testing::PendingRequest {
                method: "POST".into(),
                uri: format!("{MOCK_IPFS_ROOT_URL}/api/v0/cat?arg={}", "PLACEHOLDER_HASH")
                    .to_string(),
                headers: vec![],
                body: vec![],
                response: Some(vec![52]),
                response_headers: vec![("content-type".into(), "text/plain".into())],
                sent: true,
                ..Default::default()
            });
        }
        MockType::IpfsDown => {
            // NOTE: generic circuits path starts with IPFS CAT to get the Verilog from IPFS; and 2nd call cf below.
            // For display circuits, the only call to IPFS is at the end after generation then serialize.
            if is_generic {
                // Ideally we would not need to mock a request to test a server down
                // but cf "/primitives/core/src/offchain/testing.rs:304:21" it is a "panic!"
                state.write().expect_request(testing::PendingRequest {
                    method: "POST".into(),
                    uri: format!("{MOCK_IPFS_ROOT_URL}/api/v0/cat?arg={}", "PLACEHOLDER_HASH")
                        .to_string(),
                    headers: vec![],
                    body: vec![],
                    response: Some(r#"PLAHOLDER_SERVER_DOWN"#.into()),
                    // NOT setting it will make it throw "ResponseMissingContentTypeHeader" in interstellar_http_client
                    response_headers: vec![],
                    sent: true,
                    ..Default::default()
                });
            } else {
                // Ideally we would not need to mock a request to test a server down
                // but cf "/primitives/core/src/offchain/testing.rs:304:21" it is a "panic!"
                state.write().expect_request(testing::PendingRequest {
                    method: "POST".into(),
                    uri: format!("{MOCK_IPFS_ROOT_URL}/api/v0/add").to_string(),
                    headers: vec![(
                        "Content-Type".into(),
                        "multipart/form-data;boundary=\"boundary\"".into(),
                    )],
                    body: interstellar_http_client::new_multipart_body_bytes(
                        OVERWRITTEN_SERIALIZED_IPFS_ADD,
                    ),
                    response: Some(r#"PLAHOLDER_SERVER_DOWN"#.into()),
                    // NOT setting it will make it throw "ResponseMissingContentTypeHeader" in interstellar_http_client
                    response_headers: vec![],
                    sent: true,
                    ..Default::default()
                });
            }
        }
    };

    (t,)
}
