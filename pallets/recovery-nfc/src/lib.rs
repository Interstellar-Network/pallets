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
    use frame_support::sp_runtime::traits::StaticLookup;
    use frame_system::pallet_prelude::*;
    use sp_std::vec::Vec;
    use std::str::FromStr;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_recovery::Config + 'static {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    /// Easy way to make a link b/w a "message" and "pinpad" circuits
    // TODO(recovery) update structs and corresponding Map
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
    pub struct DisplayValidationPackage {
        // usually only 2-4 digits for the message, and always 10 for the pinpad
        // but we can take some margin
        pub message_digits: BoundedVec<u8, ConstU32<10>>,
        pub pinpad_digits: BoundedVec<u8, ConstU32<10>>,
    }

    /// Store account -> ipfs_hash -> CircuitServerMetadata; typically at least the OTP/digits/permutation
    /// This will be checked against user input to pass/fail the current tx
    // #[pallet::storage]
    // #[pallet::getter(fn circuit_server_metadata_map)]
    // pub(super) type CircuitServerMetadataMap<T: Config> =
    //     StorageMap<_, Twox128, CircuitServerMetadataKey<T>, CircuitServerMetadata, ValueQuery>;
    #[pallet::storage]
    #[pallet::getter(fn circuit_server_metadata_map)]
    pub(super) type CircuitServerMetadataMap<T: Config> = StorageDoubleMap<
        _,
        Twox128,
        T::AccountId,
        Twox128,
        // 32 b/c IPFS hash is 256 bits = 32 bytes
        // But due to encoding(??) in practice it is 46 bytes(checked with debugger)
        // TODO for now we reference the whole "DisplayStrippedCircuitsPackage" by just using the message_pgarbled_cid;
        //      do we need to use the 4 field as the key?
        BoundedVec<u8, ConstU32<64>>,
        //  Struct containing both message_digits and pinpad_digits
        DisplayValidationPackage,
        // TODO?
        // ValueQuery,
    >;

    /// The current storage version.
    const STORAGE_VERSION: frame_support::traits::StorageVersion =
        frame_support::traits::StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(PhantomData<T>);

    // Pallets use events to inform users when important changes are made.
    // https://docs.substrate.io/v3/runtime/events-and-errors
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// One of those is emitted at the end of the tx validation
        TxPass {
            account_id: T::AccountId,
        },
        TxFail {
            account_id: T::AccountId,
        },
        /// DEBUG ONLY
        DEBUGNewDigitsSet {
            message_digits: Vec<u8>,
            pinpad_digits: Vec<u8>,
        },
    }

    // Errors inform users that something went wrong.
    #[derive(Clone)]
    #[pallet::error]
    pub enum Error<T> {
        /// Errors should have helpful documentation associated with them.
        StorageOverflow,
    }

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// and if everything is right, in the end forwards to `pallet_recovery::create_recovery`
        ///
        #[pallet::call_index(0)]
        #[pallet::weight(10_000)] // TODO + T::DbWeight::get().writes(1)
        pub fn create_recovery_nfc(
            origin: OriginFor<T>,
            hashed_nfc_tag: Vec<u8>,
        ) -> DispatchResult {
            // Check that the extrinsic was signed and get the signer.
            // This function will return an error if the extrinsic is not signed.
            // https://docs.substrate.io/v3/runtime/origins
            let who = ensure_signed(origin.clone())?;

            // TODO(recovery) store hash into Storage

            let who_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(who.clone());
            match pallet_recovery::Recoverable::<T>::get(&who) {
                Some(existing_recovery_config) => {
                    // TODO(recovery)? there is already a Recovery config set up
                    // in this case we simply try do "do nothing"
                    // ie DO NOT modify friends,threshold,delay_period
                    // TODO(recovery)? or do nothing at all?
                    // https://github.com/paritytech/polkadot-sdk/blob/1835c091c42456e8df3ecbf0a94b7b88c395f623/substrate/frame/society/src/benchmarking.rs#L63
                    // let new_friends = existing_recovery_config.friends + who;
                    // pallet_recovery::Pallet::<T>::create_recovery(
                    //     origin,
                    //     new_friends,
                    //     existing_recovery_config.threshold + 1,
                    //     existing_recovery_config.delay_period
                    // )?;
                }
                None => {
                    // TODO(recovery)? store a new "fake account" and ADD it to the existing Recovery `friends`
                    // MAYBE we can directly use the current `origin`/`who` for this?
                    // what are the cons? is this dangerous? what happens when using Social Recovery with self?
                    // let ten_blocks = <frame_system::Pallet<T>>::block_number()
                    //     - <frame_system::Pallet<T>>::block_number();
                    // let ten_blocks: T::BlockNumber = 10;
                    let ten_blocks = T::BlockNumber::from_str("10").unwrap_or_default();
                    pallet_recovery::Pallet::<T>::create_recovery(
                        origin.clone(),
                        vec![who],
                        1,
                        ten_blocks,
                    )?;
                }
            }

            // TODO(recovery) should probably call initiate_recovery (in both cases?)
            // Needed else we get Error `NotStarted`
            // TODO(recovery) SHOULD NOT call if already initiated
            pallet_recovery::Pallet::<T>::initiate_recovery(origin, who_lookup)?;

            Ok(())
        }

        // TODO(recovery) add call forwarding to `initiate_recovery`
        // or merge with `vouch_recovery` and do some kind of "initiate if needed"?

        /// Check if NFC S/N is associated with the current account(among other things)
        /// and if everything is right, in the end forwards to `pallet_recovery::vouch_recovery`
        ///
        #[pallet::call_index(1)]
        #[pallet::weight(10_000)] // TODO + T::DbWeight::get().writes(1)
        pub fn vouch_with_nfc_tag(origin: OriginFor<T>, hashed_nfc_tag: Vec<u8>) -> DispatchResult {
            // Check that the extrinsic was signed and get the signer.
            // This function will return an error if the extrinsic is not signed.
            // https://docs.substrate.io/v3/runtime/origins
            let who = ensure_signed(origin.clone())?;
            log::info!(
                "[nfc-recovery] vouch_with_nfc_tag: who = {:?}, hashed_nfc_tag = {:?}",
                &who,
                hashed_nfc_tag,
            );

            // TODO(recovery) Compare with storage
            // let display_validation_package = <CircuitServerMetadataMap<T>>::get(
            //     who.clone(),
            //     TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(ipfs_cid).unwrap(),
            // )
            // .ok_or(Error::<T>::CircuitNotFound)?;

            // TODO(recovery) do some CHECKs then `vouch_recovery` { lost: (), rescuer: () }
            //
            // https://github.com/paritytech/polkadot-sdk/blob/1835c091c42456e8df3ecbf0a94b7b88c395f623/substrate/frame/society/src/benchmarking.rs#L63
            let who_lookup: <T::Lookup as StaticLookup>::Source = T::Lookup::unlookup(who.clone());
            pallet_recovery::Pallet::<T>::vouch_recovery(origin, who_lookup.clone(), who_lookup)?;

            // TODO(recovery)
            // if display_validation_package.message_digits == computed_inputs_from_permutation {
            //     log::info!("[nfc-recovery] TxPass",);
            //     crate::Pallet::<T>::deposit_event(Event::TxPass { account_id: who });
            //     // TODO on success: call next step/callback (ie pallet-tx-XXX)
            // } else {
            //     log::info!("[nfc-recovery] TxFail",);
            //     crate::Pallet::<T>::deposit_event(Event::TxFail { account_id: who });
            //     // DO NOT return an Err; that would rollback the tx and allow the user to retry
            //     // this is NOT what we want!
            //     // We only want to retry if the input are invalid(eg not in [0-9]) NOT if a wrong code is given
            //     //
            //     // TODO in this case we SHOULD NOT allow the user to retry; ie cleanup Storage etc
            // }

            Ok(())
        }
    }
}
