#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/v3/runtime/frame>
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config + 'static {}

    #[derive(
        Clone,
        Encode,
        Decode,
        Eq,
        PartialEq,
        RuntimeDebug,
        Default,
        scale_info::TypeInfo,
        MaxEncodedLen,
    )]
    pub struct TxResultPackage {
        /// message_pgarbled_cid: currently used to uniquely identify a "circuit package"
        /// SHOULD match the second key of "type CircuitServerMetadataMap" in interstellar-pallets/pallets/tx-validation/src/lib.rs
        /// 32 b/c IPFS hash is 256 bits = 32 bytes
        /// But due to encoding(??) in practice it is 46 bytes(checked with debugger), and we take some margin
        pub(crate) message_pgarbled_cid: BoundedVec<u8, ConstU32<64>>,
        /// SHOULD (roughly) match the Event emitted by "fn check_input" in interstellar-pallets/pallets/tx-validation/src/lib.rs
        pub(crate) result: TxResult,
    }

    /// Store account_id -> list(ipfs_cids);
    /// That represents the "history size" for a given Account
    const MAX_NUMBER_TX_RESULTS_PER_ACCOUNT: u32 = 16;

    type TxResultsType = BoundedVec<TxResultPackage, ConstU32<MAX_NUMBER_TX_RESULTS_PER_ACCOUNT>>;

    /// Store account -> List of tx results;
    #[pallet::storage]
    #[pallet::getter(fn tx_results_map)]
    pub(super) type TxResultsMap<T: Config> =
        StorageMap<_, Twox128, T::AccountId, BoundedVec<TxResultPackage, ConstU32<16>>>;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[derive(
        Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,
    )]
    pub enum TxResult {
        /// One of those is emitted at the end of the tx validation
        TxPass,
        TxFail,
    }

    impl Default for TxResult {
        fn default() -> Self {
            TxResult::TxFail
        }
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {}

    /// for now we reference the whole "DisplayStrippedCircuitsPackage" by just using the message_pgarbled_cid
    /// so we only pass "message_pgarbled_cid"
    /// It SHOULD always match what "fn check_input"(pallet-tx-validation) is using as key!
    fn store_tx_result<T: Config>(
        who: &T::AccountId,
        message_pgarbled_cid: Vec<u8>,
        result: TxResult,
    ) -> DispatchResult {
        log::info!(
            "[tx-registry] store_tx_result: who = {:?}, message_pgarbled_cid = {:?}, result = {:?}",
            who,
            sp_std::str::from_utf8(&message_pgarbled_cid).expect("message_pgarbled_cid utf8"),
            &result,
        );

        // and update our internal map of "tx history"
        // "append if exists, create if not"
        // TODO done in two steps, is there a way to do it atomically?
        let mut current_tx_history: TxResultsType =
            <TxResultsMap<T>>::try_get(&who).unwrap_or_default();
        current_tx_history
            .try_push(TxResultPackage {
                message_pgarbled_cid: TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(
                    message_pgarbled_cid,
                )
                .unwrap(),
                result: result,
            })
            .unwrap();
        <TxResultsMap<T>>::insert(who, current_tx_history);

        log::info!(
            "[tx-registry] store_tx_result: done! [{:?}]",
            <TxResultsMap<T>>::try_get(&who).unwrap_or_default()
        );

        Ok(())
    }

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// IMPORTANT: directly calling "fn store_tx_result"(not the Call) or directly modifying the Storage
        /// from the `integritee-worker` DOES NOT work.
        /// To be able to sync from sidechain/enclave -> parentchain, it MUST go through a Call
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn store_tx_result(
            origin: OriginFor<T>,
            message_pgarbled_cid: Vec<u8>,
            result: TxResult,
        ) -> DispatchResult {
            // Check that the extrinsic was signed and get the signer.
            // This function will return an error if the extrinsic is not signed.
            // https://docs.substrate.io/v3/runtime/origins
            let who = ensure_signed(origin)?;

            store_tx_result::<T>(&who, message_pgarbled_cid, result)
        }
    }
}
