use super::*;
use crate as pallet_recovery_nfc;
use frame_support::parameter_types;
use frame_support::traits::{ConstU16, ConstU64};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
};

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
        Balances: pallet_balances,
        Recovery: pallet_recovery,
        RecoveryNfc: pallet_recovery_nfc,
    }
);

impl system::Config for Test {
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
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    // https://github.com/paritytech/substrate/blob/8c4b84520cee2d7de53cc33cb67605ce4efefba8/frame/recovery/src/mock.rs#L66
    type AccountData = pallet_balances::AccountData<u128>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

// https://github.com/paritytech/substrate/blob/8c4b84520cee2d7de53cc33cb67605ce4efefba8/frame/recovery/src/mock.rs#L75
parameter_types! {
    pub const ExistentialDeposit: u64 = 1;
}

/// https://github.com/paritytech/substrate/blob/8c4b84520cee2d7de53cc33cb67605ce4efefba8/frame/recovery/src/mock.rs#L79
impl pallet_balances::Config for Test {
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
}

// cf /substrate/frame/recovery/src/mock.rs
parameter_types! {
    pub const ConfigDepositBase: u64 = 10;
    pub const FriendDepositFactor: u64 = 1;
    pub const RecoveryDeposit: u64 = 10;
    // Large number of friends for benchmarking.
    pub const MaxFriends: u32 = 128;
}

impl pallet_recovery::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type RuntimeCall = RuntimeCall;
    type Currency = Balances;
    type ConfigDepositBase = ConfigDepositBase;
    type FriendDepositFactor = FriendDepositFactor;
    type MaxFriends = MaxFriends;
    type RecoveryDeposit = RecoveryDeposit;
}

impl pallet_recovery_nfc::Config for Test {
    type RuntimeEvent = RuntimeEvent;
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    // https://github.com/paritytech/substrate/blob/8c4b84520cee2d7de53cc33cb67605ce4efefba8/frame/recovery/src/mock.rs#L113
    // TODO? https://github.com/paritytech/substrate/blob/033d4e86cc7eff0066cd376b9375f815761d653c/frame/babe/src/mock.rs#L347

    let mut t = frame_system::GenesisConfig::default()
        .build_storage::<Test>()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 100), (2, 100), (3, 100), (4, 100), (5, 100)],
    }
    .assimilate_storage(&mut t)
    .unwrap();

    // Need to advanced block else "panicked at 'events not registered at the genesis block'"
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
