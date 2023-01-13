use crate as pallet_ocw_garble;
use crate::*;
use codec::Decode;
use frame_support::{
    assert_ok, parameter_types,
    traits::{ConstU16, ConstU32, ConstU64},
};
use serde_json::json;
use sp_core::{
    offchain::{testing, OffchainWorkerExt, TransactionPoolExt},
    sr25519::Signature,
    H256,
};
use sp_keystore::{testing::KeyStore, KeystoreExt, SyncCryptoStore};
use sp_runtime::{
    testing::{Header, TestXt},
    traits::{BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup, Verify},
    RuntimeAppPublic,
};
use std::sync::Arc;
use tests_common::foreign_ipfs::ForeignNode;

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
    type Event = Event;
}

impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Origin = Origin;
    type Call = Call;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = sp_core::sr25519::Public;
    // type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
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

type Extrinsic = TestXt<Call, ()>;
type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

impl frame_system::offchain::SigningTypes for Test {
    type Public = <Signature as Verify>::Signer;
    type Signature = Signature;
}

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Test
where
    Call: From<LocalCall>,
{
    type OverarchingCall = Call;
    type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Test
where
    Call: From<LocalCall>,
{
    fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
        call: Call,
        _public: <Signature as Verify>::Signer,
        _account: AccountId,
        nonce: u64,
    ) -> Option<(Call, <Extrinsic as ExtrinsicT>::SignaturePayload)> {
        Some((call, (nonce, ())))
    }
}

parameter_types! {
    pub const UnsignedPriority: u64 = 1 << 20;
}

impl pallet_ocw_garble::Config for Test {
    type Event = Event;
    type RuntimeCall = Call;
    type AuthorityId = crypto::TestAuthId;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> (sp_io::TestExternalities, ForeignNode) {
    std::env::set_var("INTERSTELLAR_URI_NODE", "http://127.0.0.1:4242");

    let (offchain, state) = testing::TestOffchainExt::new();
    let mut t = sp_io::TestExternalities::default();
    t.register_extension(OffchainWorkerExt::new(offchain));

    // NOTE: PORT hardcoded in lib.rs so we can use a dynamic one
    let foreign_node = ForeignNode::new(None);
    std::env::set_var(
        "IPFS_ROOT_URL",
        format!("http://127.0.0.1:{}", foreign_node.api_port),
    );

    get_ocw_circuits_storage_value_response(&mut state.write());
    mock_ipfs_cat_response(&mut state.write(), foreign_node.api_port);

    (t, foreign_node)

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
}

/// For now b/c https://github.com/integritee-network/worker/issues/976
/// we query the storage using a HTTP request; so we need to Mock it
/// cf "fn get_ocw_circuits_storage_value"
///
/// based on https://github.com/paritytech/substrate/blob/e9b0facf70eeb08032cc7e83548c62f0b4a24bb1/frame/examples/offchain-worker/src/tests.rs#L385
fn get_ocw_circuits_storage_value_response(state: &mut testing::OffchainState) {
    let body_json = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method":"state_getStorage",
        // TODO compute this dynamically
        "params": ["0x2c644167ae9423d1f0683de9002940b8bd009489ffa75ba4c0b3f4f6fed7414b"]
    });
    let body_vec = serde_json::to_vec(&body_json).unwrap();

    state.expect_request(testing::PendingRequest {
        method: "POST".into(),
        uri: "http://127.0.0.1:4242".into(),
        // cf "fn decode_rpc_json" for the expected format
        response: Some(br#"{"id": "1", "jsonrpc": "2.0", "result": "0xb8516d626945354373524d4a7565316b5455784d5a5162694e394a794e5075384842675a346138726a6d344353776602000000b8516d5a7870436964427066624c74675534796434574a314d7654436e5539316e7867394132446137735a7069636d0a000000"}"#.to_vec()),
        sent: true,
        headers: vec![("Content-Type".into(), "application/json;charset=utf-8".into())],
        body: body_vec,
        response_headers: vec![("content-type".into(), "application/json; charset=utf-8".into())],
        ..Default::default()
    });
}

fn mock_ipfs_cat_response(state: &mut testing::OffchainState, api_port: u16) {
    state.expect_request(testing::PendingRequest {
        method: "POST".into(),
        uri: format!(
            "http://127.0.0.1:{}/api/v0/cat?arg=QmbiE5CsRMJue1kTUxMZQbiN9JyNPu8HBgZ4a8rjm4CSwf",
            api_port
        ),
        // cf "fn decode_rpc_json" for the expected format
        response: Some(
            include_bytes!("../tests/data/display_message_120x52_2digits.skcd.pb.bin").to_vec(),
        ),
        sent: true,
        headers: vec![],
        response_headers: vec![("content-type".into(), "text/plain".into())],
        ..Default::default()
    });
}
