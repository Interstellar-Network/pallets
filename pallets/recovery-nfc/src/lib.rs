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
    use frame_system::{pallet_prelude::*, RawOrigin};
    use sp_std::vec::Vec;
    use std::str::FromStr;

    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_recovery::Config + 'static {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
    }

    /// Store map: account -> "NFC S/N(Serial Number)"
    /// This will be stored during `create_recovery_nfc` and checked in `vouch_with_nfc_tag`
    /// NOTE: yes, the key is the S/N; that is b/c the AccountId from `vouch_with_nfc_tag` is NOT
    /// the one used during setup!
    #[pallet::storage]
    #[pallet::getter(fn map_account_nfc_tag)]
    pub(super) type MapAccountNfcTag<T: Config> = StorageMap<
        _,
        Twox128,
        // NFC S/N seems to be at least 4 bytes, and possibly 8-12?
        // But we store a HASH of it, so we want at 256 bits(and 512 to be future proof)
        BoundedVec<u8, ConstU32<64>>,
        T::AccountId,
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
        /// Emitted at the end of `vouch_with_nfc_tag` when everything was OK
        VouchedWithNfc { account_id: T::AccountId },
        /// Trying to call `vouch_with_nfc_tag` WITHOUT having called `create_recovery_nfc`
        /// ie there is no entry for current account in `MapAccountNfcTag`
        /// Emitted at the end of `vouch_with_nfc_tag` if the NFC S/N is not the correct one
        UnknownAccount { account_id: T::AccountId },
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {}

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // TODO(recovery) SHOULD probabably receive a 'nfc_sn' in clear; and hash in the calls

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

            match pallet_recovery::Recoverable::<T>::get(&who) {
                Some(_existing_recovery_config) => {
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
                        vec![who.clone()],
                        1,
                        ten_blocks,
                    )?;
                }
            }

            // Store hash into Storage
            // Will be compared in `vouch_with_nfc_tag`
            MapAccountNfcTag::<T>::insert(
                TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(hashed_nfc_tag).unwrap(),
                who,
            );

            Ok(())
        }

        // TODO(recovery) add call forwarding to `initiate_recovery`
        // or merge with `vouch_recovery` and do some kind of "initiate if needed"?
        // or better(cf current) make calling `initiate_recovery` a PREREQ of `vouch` which simplifies the process

        /// Check if NFC S/N is associated with the current account(among other things)
        /// and if everything is right, in the end forwards to `pallet_recovery::vouch_recovery`
        ///
        /// PREREQ
        /// - `pallet_recovery::initiate_recovery` MUST have been called before
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

            // Compare with storage
            // TODO(recovery) same question than "InvalidNfcTag" below
            // : frame_system::RawOrigin<T::AccountId>
            let lost_account_id = match <MapAccountNfcTag<T>>::get(
                TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(hashed_nfc_tag).unwrap(),
            ) {
                Some(lost_account_id) => lost_account_id,
                // match lost_account_id {
                //     frame_system::RawOrigin::Root => todo!(),
                //     frame_system::RawOrigin::Signed(lost_origin_signed) => lost_origin_signed,
                //     frame_system::RawOrigin::None => todo!(),
                // }
                None => {
                    // TODO(recovery) SHOULD this be an error? we WANT the caller to pay if the wrong NFC tag is given
                    // how does that work?
                    // DO NOT return an Err; that would rollback the tx and allow the user to retry
                    // this is NOT what we want!
                    log::info!("[nfc-recovery] InvalidNfcTag",);
                    crate::Pallet::<T>::deposit_event(Event::UnknownAccount { account_id: who });
                    return Ok(());
                }
            };

            // https://github.com/paritytech/polkadot-sdk/blob/1835c091c42456e8df3ecbf0a94b7b88c395f623/substrate/frame/society/src/benchmarking.rs#L63
            let lost_lookup: <T::Lookup as StaticLookup>::Source =
                T::Lookup::unlookup(lost_account_id.clone());

            pallet_recovery::Pallet::<T>::vouch_recovery(
                RawOrigin::from(Some(lost_account_id)).into(),
                lost_lookup.clone(),
                lost_lookup,
            )?;

            log::info!("[nfc-recovery] done!",);
            crate::Pallet::<T>::deposit_event(Event::VouchedWithNfc { account_id: who });

            Ok(())
        }
    }
}
