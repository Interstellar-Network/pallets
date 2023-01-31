#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::pallet_prelude::ConstU32;
use frame_support::pallet_prelude::MaxEncodedLen;
use frame_support::BoundedVec;
use frame_support::RuntimeDebug;

/// Easy way to make a link b/w a "message" and "pinpad" circuits
/// that way we can have ONE extrinsic that generates both in one call
///
/// It SHOULD roughly mirror pallet_ocw_circuits::DisplaySkcdPackage
#[derive(
    Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, scale_info::TypeInfo, MaxEncodedLen,
)]
pub struct DisplayStrippedCircuitsPackage {
    // 32 b/c IPFS hash is 256 bits = 32 bytes
    // But due to encoding(??) in practice it is 46 bytes(checked with debugger), and we take some margin
    pub message_pgarbled_cid: BoundedVec<u8, ConstU32<64>>,
    pub message_packmsg_cid: BoundedVec<u8, ConstU32<64>>,
    pub pinpad_pgarbled_cid: BoundedVec<u8, ConstU32<64>>,
    pub pinpad_packmsg_cid: BoundedVec<u8, ConstU32<64>>,
    /// needed for UI/UX purposes
    /// Used in repo `wallet-app`; DO NOT remove "pub"!
    pub message_nb_digits: u32,
}
