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
use tests_utils::foreign_ipfs;

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

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> (sp_io::TestExternalities, foreign_ipfs::ForeignNode) {
    // MOCK "integritee-node" RPC
    let mock_server_uri_node = MockServer::start();

    std::env::set_var("INTERSTELLAR_URI_NODE", mock_server_uri_node.base_url());

    let (offchain, state) = testing::TestOffchainExt::new();
    let mut t = sp_io::TestExternalities::default();
    t.register_extension(OffchainWorkerExt::new(offchain));

    // NOTE: PORT hardcoded in lib.rs so we can use a dynamic one
    let (foreign_node, _ipfs_reference_client) = foreign_ipfs::run_ipfs_in_background(None);
    std::env::set_var(
        "IPFS_ROOT_URL",
        format!("http://127.0.0.1:{}", foreign_node.api_port),
    );

    mock_ocw_circuits_storage_value_response(&mock_server_uri_node);

    (t, foreign_node)
}

/// For now b/c https://github.com/integritee-network/worker/issues/976
/// we query the storage using a HTTP request; so we need to Mock it
/// cf "fn get_ocw_circuits_storage_value"
///
/// based on https://github.com/paritytech/substrate/blob/e9b0facf70eeb08032cc7e83548c62f0b4a24bb1/frame/examples/offchain-worker/src/tests.rs#L385
fn mock_ocw_circuits_storage_value_response(server: &MockServer) {
    let body_json = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method":"state_getStorage",
        // TODO compute this dynamically
        "params": ["0x2c644167ae9423d1f0683de9002940b8bd009489ffa75ba4c0b3f4f6fed7414b"]
    });
    let body = serde_json::to_string(&body_json).unwrap();

    server.mock(|when, then| {
        when.method(POST)
            .path("/")
            .header("Content-Type", "application/json;charset=utf-8")
            .body(&body)
            ;
        then.status(200)
            .header("content-type", "application/json; charset=utf-8")
            // cf "fn decode_rpc_json" for the expected format
            // MUST match the param passed to "mock_ipfs_cat_response"
            .body(br#"{"id": "1", "jsonrpc": "2.0", "result": "0xb8516d626945354373524d4a7565316b5455784d5a5162694e394a794e5075384842675a346138726a6d344353776602000000b8516d5a7870436964427066624c74675534796434574a314d7654436e5539316e7867394132446137735a7069636d0a000000"}"#.to_vec());
    });
}
