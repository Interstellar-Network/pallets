#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

extern crate alloc;

use alloc::string::String;
use alloc::string::ToString;
use codec::{Decode, Encode};
use frame_system::ensure_signed;
use frame_system::offchain::AppCrypto;
use frame_system::offchain::CreateSignedTransaction;
use rand::seq::SliceRandom;
use rand::Rng;
use rand_chacha::{rand_core::SeedableRng, ChaChaRng};
use scale_info::prelude::*;
use serde::Deserialize;
use serde_json::json;
use sp_core::crypto::KeyTypeId;
use sp_runtime::traits::BlockNumberProvider;
use sp_runtime::transaction_validity::InvalidTransaction;
use sp_std::borrow::ToOwned;
use sp_std::str;
use sp_std::vec::Vec;

use interstellar_http_client::SendRequest;
use interstellar_ipfs_client::IpfsClient;

pub use pallet::*;

// TODO(interstellar) remove; and cascade
struct GarbleAndStripIpfsReply {
    pgarbled_cid: String,
}

/// TEST ONLY "hook"
/// Because the tests need the full body bytes to mock correctly...
///
/// cf https://github.com/paritytech/substrate/pull/12307/files
///
#[cfg(test)]
pub trait MyTestCallback {
    fn my_test_hook(input: Vec<u8>) -> Vec<u8> {
        input
    }
}

/// Empty implementation in case no callbacks are required.
#[cfg(test)]
impl MyTestCallback for () {}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    pub use circuits_storage_common::DisplayStrippedCircuitsPackage;

    /// Defines application identifier for crypto keys of this module.
    ///
    /// Every module that deals with signatures needs to declare its unique identifier for
    /// its crypto keys.
    /// When an offchain worker is signing transactions it's going to request keys from type
    /// `KeyTypeId` via the keystore to sign the transaction.
    /// The keys can be inserted manually via RPC (see `author_insertKey`).
    pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"garb");

    /// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrapper.
    /// We can utilize the supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
    /// them with the pallet-specific identifier.
    pub mod crypto {
        use crate::KEY_TYPE;
        use sp_core::sr25519::Signature as Sr25519Signature;
        use sp_runtime::{
            app_crypto::{app_crypto, sr25519},
            traits::Verify,
            MultiSignature, MultiSigner,
        };
        use sp_std::prelude::*;

        app_crypto!(sr25519, KEY_TYPE);

        pub struct TestAuthId;
        // implemented for runtime
        impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
            type RuntimeAppPublic = Public;
            type GenericSignature = sp_core::sr25519::Signature;
            type GenericPublic = sp_core::sr25519::Public;
        }

        // implemented for mock runtime in test
        impl
            frame_system::offchain::AppCrypto<
                <Sr25519Signature as Verify>::Signer,
                Sr25519Signature,
            > for TestAuthId
        {
            type RuntimeAppPublic = Public;
            type GenericSignature = sp_core::sr25519::Signature;
            type GenericPublic = sp_core::sr25519::Public;
        }
    }

    // NOTE: pallet_tx_validation::Config b/c we want to Call its extrinsics from the internal extrinsics
    // (callback from offchain_worker)
    #[pallet::config]
    pub trait Config:
        frame_system::Config
        + CreateSignedTransaction<Call<Self>>
        + pallet_ocw_circuits::Config
        + pallet_tx_validation::Config
        + 'static // TODO TOREMOVE
    // + pallet_timestamp::Config
    {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// The overarching dispatch call type.
        type RuntimeCall: From<Call<Self>>;
        /// The identifier type for an offchain worker.
        type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
        #[cfg(test)]
        type HookCallGrpGarbleAndStripSerializedPackageForEval: MyTestCallback;
    }

    pub type PendingCircuitsType = BoundedVec<
        DisplayStrippedCircuitsPackage,
        ConstU32<MAX_NUMBER_PENDING_CIRCUITS_PER_ACCOUNT>,
    >;

    /// Store account_id -> list(ipfs_cids);
    /// That represents the "list of pending txs" for a given Account
    const MAX_NUMBER_PENDING_CIRCUITS_PER_ACCOUNT: u32 = 16;
    #[pallet::storage]
    #[pallet::getter(fn get_pending_circuits_for_account)]
    pub(super) type AccountToPendingCircuitsMap<T: Config> = StorageMap<
        _,
        Twox128,
        // key: AccountId
        T::AccountId,
        PendingCircuitsType,
        ValueQuery,
    >;

    #[pallet::storage]
    pub(super) type Nonce<T: Config> = StorageValue<_, u64, ValueQuery>;

    // // TODO decouple cf https://substrate.stackexchange.com/questions/3354/access-storage-map-from-another-pallet-without-trait-pallet-config
    // pub struct DisplaySkcdPackageValueCopyPrefix;
    // impl frame_support::traits::StorageInstance for DisplaySkcdPackageValueCopyPrefix {
    //     fn pallet_prefix() -> &'static str {
    //         "OcwCircuits"
    //     }

    //     const STORAGE_PREFIX: &'static str = "DisplaySkcdPackageValue";
    // }

    // #[pallet::storage]
    // pub type DisplaySkcdPackageValueCopy<T> =
    //     StorageValue<_, pallet_ocw_circuits::DisplaySkcdPackage, ValueQuery>;

    /// The current storage version.
    const STORAGE_VERSION: frame_support::traits::StorageVersion =
        frame_support::traits::StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        // Sent at the end of the offchain_worker(ie it is an OUTPUT)
        NewGarbledIpfsCid(Vec<u8>),
        // Strip version: (one IPFS cid for the circuit), for both mesage and pinpad
        NewGarbleAndStrippedIpfsCid {
            message_pgarbled_cid: Vec<u8>,
            pinpad_pgarbled_cid: Vec<u8>,
        },
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        // Error returned when not sure which ocw function to executed
        UnknownOffchainMux,

        // Error returned when making signed transactions in off-chain worker
        NoLocalAcctForSigning,
        OffchainSignedTxError,

        // Error returned when making unsigned transactions in off-chain worker
        OffchainUnsignedTxError,

        // Error returned when making unsigned transactions with signed payloads in off-chain worker
        OffchainUnsignedTxSignedPayloadError,

        // Error returned when fetching github info
        MissingSkcdCircuitsError,
        HttpFetchingError,
        DeserializeError,
        IpfsClientCreationError,
        IpfsCallError,
        GarblerError,
        Utf8Error,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        // TODO TOREMOVE "fn offchain_worker" not used in TEE
        /*
        /// Offchain Worker entry point.
        ///
        /// By implementing `fn offchain_worker` you declare a new offchain worker.
        /// This function will be called when the node is fully synced and a new best block is
        /// succesfuly imported.
        /// Note that it's not guaranteed for offchain workers to run on EVERY block, there might
        /// be cases where some blocks are skipped, or for some the worker runs twice (re-orgs),
        /// so the code should be able to handle that.
        /// You can use `Local Storage` API to coordinate runs of the worker.
        fn offchain_worker(block_number: T::BlockNumber) {
            log::info!("[ocw-garble] Hello from pallet-ocw-garble.");

            let result = Self::process_if_needed(block_number);

            if let Err(e) = result {
                log::error!("[ocw-garble] offchain_worker error: {:?}", e);
            }
        }
        */
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        /// Validate unsigned call to this module.
        ///
        /// By default unsigned transactions are disallowed, but implementing the validator
        /// here we make sure that some particular calls (the ones produced by offchain worker)
        /// are being whitelisted and marked as valid.
        fn validate_unsigned(
            _source: TransactionSource,
            _call: &Self::Call,
        ) -> TransactionValidity {
            // TODO?
            InvalidTransaction::Call.into()
        }
    }

    impl<T: Config> Pallet<T> {
        /// Get the Storage using an RPC
        /// Needed b/c Storage access from a worker is not yet functional: https://github.com/integritee-network/worker/issues/976
        /// [FAIL cf core/rpc-client/src/direct_client.rs and cli/src/trusted_operation.rs
        ///     -> issue with sgx_tstd + std]
        /// cf https://github.com/scs/substrate-api-client/blob/master/examples/example_get_storage.rs
        fn get_ocw_circuits_storage_value_rpc(
        ) -> Result<circuits_storage_common::DisplaySkcdPackage, Error<T>> {
            // TODO? use proper struct to encode the request
            let body_json = json!({
                "jsonrpc": "2.0",
                "id": "1",
                "method":"state_getStorage",
                // TODO compute this dynamically
                // You can get it using eg https://polkadot.js.org/apps/#/chainstate;
                // then select "ocwCircuits" and then the correct storage entry.
                "params": [compute_storage_hash_hex_for_rpc("OcwCircuits", "DisplaySkcdPackageValue")]
            });

            let endpoint = get_node_uri();

            let (resp_bytes, resp_content_type) =
                interstellar_http_client::ClientHttpReq::send_request(
                    Some(bytes::Bytes::from(serde_json::to_vec(&body_json).unwrap())),
                    &endpoint,
                    &interstellar_http_client::MyRequestMethod::Post,
                    Some(&interstellar_http_client::MyContentType::Json),
                    core::time::Duration::from_millis(2_000),
                )
                .map_err(|e| {
                    log::error!(
                        "[ocw-garble] call_grpc_garble error: {:?} [{:?}]",
                        e,
                        endpoint
                    );
                    <Error<T>>::HttpFetchingError
                })?;

            let response: circuits_storage_common::DisplaySkcdPackage =
                interstellar_http_client::decode_rpc_json(&resp_bytes, &resp_content_type)
                    .map_err(|e| {
                        log::error!("[ocw-circuits] call_grpc_generic error: {:?}", e);
                        <Error<T>>::DeserializeError
                    })?;
            log::info!(
                "[ocw-garble] get_ocw_circuits_storage_value response : {:?}",
                response
            );

            Ok(response)
        }

        /// Read the Storage from OcwCircuits using the public getter
        ///
        /// NOTE: check git history if for some reason you need to go back to either reading the
        /// storage directly(using sp_io::hashing::twox_128) or via RPC.
        ///
        /// 2023-02-02: still broken? cf https://github.com/Interstellar-Network/roadmap/issues/73
        fn get_ocw_circuits_storage_value(
        ) -> Result<circuits_storage_common::DisplaySkcdPackage, Error<T>> {
            match pallet_ocw_circuits::get_display_circuits_package::<T>() {
                Ok(circuit) => Ok(circuit),
                Err(_) => {
                    log::warn!("[ocw-garble] get_ocw_circuits_storage_value: storage COULD NOT be read! Fallback to RPC...");
                    Self::get_ocw_circuits_storage_value_rpc()
                        .map_err(|_err| <Error<T>>::MissingSkcdCircuitsError)
                }
            }
        }

        // TODO TOREMOVE #[pallet::weight(10000)]
        pub fn callback_new_garbled_and_strip_signed(
            who: T::AccountId,
            message_pgarbled_cid: Vec<u8>,
            message_digits: Vec<u8>,
            pinpad_pgarbled_cid: Vec<u8>,
            pinpad_digits: Vec<u8>,
        ) -> DispatchResult {
            // TODO TOREMOVE
            // let who = ensure_signed(origin.clone())?;

            log::info!(
                "[ocw-garble] callback_new_garbled_and_strip_signed: {:?} ; {:?} for {:?}",
                sp_std::str::from_utf8(&message_pgarbled_cid)
                    .map_err(|_err| <Error<T>>::Utf8Error)?,
                sp_std::str::from_utf8(&pinpad_pgarbled_cid)
                    .map_err(|_err| <Error<T>>::Utf8Error)?,
                who
            );

            Self::deposit_event(Event::NewGarbleAndStrippedIpfsCid {
                message_pgarbled_cid: message_pgarbled_cid.clone(),
                pinpad_pgarbled_cid: pinpad_pgarbled_cid.clone(),
            });

            // store the metadata using the pallet-tx-validation
            // (only in "garble+strip" mode b/c else it makes no sense)
            // TODO? Call?
            // pallet_tx_validation::Call::<T>::store_metadata {
            //     ipfs_cid: pgarbled_cid,
            //     circuit_digits: circuit_digits,
            // };
            pallet_tx_validation::store_metadata_aux::<T>(
                &who,
                message_pgarbled_cid.clone(),
                message_digits.clone(),
                pinpad_digits,
            )
            .expect("store_metadata_aux failed!");

            // and update our internal map of pending circuits for the given account
            // this is USED via RPC by the app, not directly!
            // "append if exists, create if not"
            // TODO done in two steps, is there a way to do it atomically?
            let mut current_pending_circuits: PendingCircuitsType =
                <AccountToPendingCircuitsMap<T>>::try_get(&who).unwrap_or_default();
            current_pending_circuits
                .try_push(DisplayStrippedCircuitsPackage {
                    message_pgarbled_cid: TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(
                        message_pgarbled_cid,
                    )
                    .unwrap(),
                    pinpad_pgarbled_cid: TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(
                        pinpad_pgarbled_cid,
                    )
                    .unwrap(),
                    message_nb_digits: message_digits.len().try_into().unwrap(),
                })
                .unwrap();
            <AccountToPendingCircuitsMap<T>>::insert(who, current_pending_circuits);

            log::info!("[ocw-garble] callback_new_garbled_and_strip_signed: done!");

            Ok(())
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        // TODO TOREMOVE
        /*
        #[pallet::weight(10000)]
        pub fn garble_standard_signed(origin: OriginFor<T>, skcd_cid: Vec<u8>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            log::info!(
                "[ocw-garble] garble_standard_signed: ({}, {:?})",
                sp_std::str::from_utf8(&skcd_cid).map_err(|err| <Error<T>>::Utf8Error)?,
                who
            );

            Self::append_or_replace_skcd_hash(
                GrpcCallKind::GarbleStandard,
                Some(skcd_cid),
                None,
                None,
                None,
                None,
                None,
            );

            Ok(())
        }
        */

        #[pallet::call_index(0)]
        #[pallet::weight(10000)]
        pub fn garble_and_strip_display_circuits_package_signed(
            origin: OriginFor<T>,
            tx_msg: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            log::info!(
                "[ocw-garble] garble_and_strip_display_circuits_package_signed: ({:?} for {:?})",
                sp_std::str::from_utf8(&tx_msg).map_err(|_err| <Error<T>>::Utf8Error)?,
                who
            );

            // read DisplayCircuitsPackageValue directly from ocw-circuits
            //
            // FAIL, even with "key_hashes.push(storage_value_key("OcwCircuits", "DisplaySkcdPackageValue"))"
            // let display_circuits_package =
            //     pallet_ocw_circuits::Pallet::<T>::get_display_circuits_package()
            //         .expect("display_circuits_package not ready!");
            //
            // let display_circuits_package = <pallet_ocw_circuits::Pallet<T> as Trait>::DisplaySkcdPackageValue::<T>::get();
            // // CHECK: error-out if both fields are not set
            // if display_circuits_package.message_skcd_server_metadata_nb_digits == 0
            //     || display_circuits_package.pinpad_skcd_server_metadata_nb_digits == 0
            // {
            //     return Err(<Error<T>>::DisplaySkcdPackageValueError);
            // }
            //
            // let display_circuits_package = Self::get_ocw_circuits_storage_value().unwrap();
            //
            // let display_circuits_package = <DisplaySkcdPackageValueCopy<T>>::get();
            //
            let display_circuits_package = Self::get_ocw_circuits_storage_value()?;

            log::info!(
                "[ocw-garble] display_circuits_package: ({:?},{:?}) ({:?},{:?})",
                sp_std::str::from_utf8(&display_circuits_package.message_skcd_cid)
                    .map_err(|_err| <Error<T>>::Utf8Error)?,
                display_circuits_package.message_skcd_server_metadata_nb_digits,
                sp_std::str::from_utf8(&display_circuits_package.pinpad_skcd_cid)
                    .map_err(|_err| <Error<T>>::Utf8Error)?,
                display_circuits_package.pinpad_skcd_server_metadata_nb_digits,
            );

            // Generate random digits
            // FAIL does not seem to be random in enclave?
            // cf https://github.com/paritytech/substrate/blob/master/frame/lottery/src/lib.rs#L506
            // let nonce = Self::get_and_increment_nonce();
            // let (random_seed, _) = T::MyRandomness::random(&nonce);
            // random_seed is a Hash so 256 -> 32 u8 is fine
            // so we have more than enough
            // TODO we could(SHOULD) split "random_seed" 4 bits by 4 bits b/c that is enough for [0-10] range
            // let random_seed = <[u8; 32]>::decode(&mut random_seed.as_ref())
            //     .expect("secure hashes should always be bigger than u32; qed");

            // https://github.com/paritytech/substrate/blob/master/frame/society/src/lib.rs#L1420
            // TODO is ChaChaRng secure? (or at least good enough)
            let mut rng = ChaChaRng::from_entropy();

            // typically we need (2-4) digits for the message
            // and 10 digits(NOT u8) for the pinpad
            // MUST SHUFFLE the pinpad digits, NOT randomize them
            // each digit from 0 to 10 (included!) MUST be in the final "digits"
            let mut pinpad_digits: Vec<u8> =
                (0..display_circuits_package.pinpad_skcd_server_metadata_nb_digits as u8).collect();
            pinpad_digits.shuffle(&mut rng);
            let message_digits: Vec<u8> =
                (0..display_circuits_package.message_skcd_server_metadata_nb_digits as u8)
                    .map(|_| rng.gen_range(0..10))
                    .collect();
            log::info!(
                "[ocw-garble] pinpad_digits: {:?}, message_digits: {:?}",
                pinpad_digits,
                message_digits,
            );

            // Self::append_or_replace_skcd_hash(
            //     GrpcCallKind::GarbleAndStrip,
            //     // optional: only if GrpcCallKind::GarbleStandard
            //     None,
            //     // optional: only if GrpcCallKind::GarbleAndStrip
            //     Some(display_circuits_package.message_skcd_cid.to_vec()),
            //     Some(display_circuits_package.pinpad_skcd_cid.to_vec()),
            //     Some(tx_msg),
            //     Some(message_digits),
            //     Some(pinpad_digits),
            // );
            //
            // FAIL: apparently "fn offchain_worker" is NOT called?
            let result_grpc_call = Self::call_grpc_garble_and_strip(
                display_circuits_package.message_skcd_cid.to_vec(),
                display_circuits_package.pinpad_skcd_cid.to_vec(),
                tx_msg,
                message_digits,
                pinpad_digits,
            )?;

            let (message_reply, message_digits, pinpad_reply, pinpad_digits) =
                match result_grpc_call {
                    GrpcCallReplyKind::GarbleAndStrip(
                        message_reply,
                        message_digits,
                        pinpad_reply,
                        pinpad_digits,
                    ) => (message_reply, message_digits, pinpad_reply, pinpad_digits),
                };

            // TODO TOREMOVE
            // Self::finalize_grpc_call(result_grpc_call);

            Self::callback_new_garbled_and_strip_signed(
                who,
                message_reply.pgarbled_cid.bytes().collect(),
                message_digits.to_vec(),
                pinpad_reply.pgarbled_cid.bytes().collect(),
                pinpad_digits.to_vec(),
            )?;

            Ok(())
        }

        /// Called at the end of offchain_worker to publish the result
        /// Not meant to be called by a user
        // TODO use "with signed payload" and check if expected key?
        // TODO TOREMOVE
        #[pallet::call_index(1)]
        #[pallet::weight(10000)]
        pub fn callback_new_garbled_signed(
            origin: OriginFor<T>,
            pgarbled_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            log::info!(
                "[ocw-garble] callback_new_garbled_signed: ({:?},{:?})",
                sp_std::str::from_utf8(&pgarbled_cid).map_err(|_err| <Error<T>>::Utf8Error)?,
                who
            );

            Self::deposit_event(Event::NewGarbledIpfsCid(pgarbled_cid));
            Ok(())
        }
    }

    #[derive(Debug, Deserialize, Encode, Decode, Default)]
    enum GrpcCallKind {
        #[default]
        GarbleStandard,
        GarbleAndStrip,
    }

    // reply type for each GrpcCallKind
    enum GrpcCallReplyKind {
        /// two reply b/c we call the same endpoint twice: one for message, then one for pinpad
        /// param Vec<u8> = "digits"; generated randomly in "garble_and_strip_display_circuits_package_signed"
        ///   and passed all the way around
        GarbleAndStrip(
            crate::GarbleAndStripIpfsReply,
            Vec<u8>,
            crate::GarbleAndStripIpfsReply,
            Vec<u8>,
        ),
    }

    impl<T: Config> Pallet<T> {
        /// Regroup the 2 calls to API_ENDPOINT_GARBLE_STRIP_URL in one
        fn call_grpc_garble_and_strip(
            message_skcd_ipfs_cid: Vec<u8>,
            pinpad_skcd_ipfs_cid: Vec<u8>,
            tx_msg: Vec<u8>,
            message_digits: Vec<u8>,
            pinpad_digits: Vec<u8>,
        ) -> Result<GrpcCallReplyKind, Error<T>> {
            // TODO pass correct params for pinpad and message
            let message_reply = call_grpc_garble_and_strip_one::<T>(
                message_skcd_ipfs_cid,
                tx_msg,
                message_digits.clone(),
            )?;
            let pinpad_reply = call_grpc_garble_and_strip_one::<T>(
                pinpad_skcd_ipfs_cid,
                vec![],
                pinpad_digits.clone(),
            )?;

            // TODO pass correct params for pinpad and message
            Ok(GrpcCallReplyKind::GarbleAndStrip(
                message_reply,
                message_digits,
                pinpad_reply,
                pinpad_digits,
            ))
        }

        // TODO TOREMOVE does not work in TEE?
        /*
        /// Called at the end of process_if_needed/offchain_worker
        /// Publish the result back via send_signed_transaction(and Event)
        ///
        /// param: result_grpc_call: returned by call_grpc_garble_and_strip/call_grpc_garble
        //
        // [2022-09-12T14:38:33Z WARN  sp_io::crypto] crypto::sr25519_public_key unimplemented
        // [2022-09-12T14:38:33Z WARN  sp_io::crypto] crypto::sr25519_public_key unimplemented
        // [2022-09-12T14:38:33Z WARN  ita_sgx_runtime] Unable to create signed payload: <wasm:stripped>
        // [2022-09-12T14:38:33Z INFO  pallet_ocw_garble::pallet] [ocw-garble] finalize_grpc_call sent number : 0
        //
        // fn finalize_grpc_call(result_grpc_call: Result<GrpcCallReplyKind, Error<T>>) {
        //     match result_grpc_call {
        //         Ok(result_reply) => {
        //             // Using `send_signed_transaction` associated type we create and submit a transaction
        //             // representing the call we've just created.
        //             // `send_signed_transaction()` return type is `Option<(Account<T>, Result<(), ()>)>`. It is:
        //             //   - `None`: no account is available for sending transaction
        //             //   - `Some((account, Ok(())))`: transaction is successfully sent
        //             //   - `Some((account, Err(())))`: error occurred when sending the transaction
        //             let signer = Signer::<T, <T as Config>::AuthorityId>::all_accounts();
        //             if !signer.can_sign() {
        //                 log::error!(
        //                     "[ocw-garble] No local accounts available. Consider adding one via `author_insertKey` RPC[ALTERNATIVE DEV ONLY check 'if config.offchain_worker.enabled' in service.rs]"
        //                 );
        //             }

        //             let results = signer.send_signed_transaction(|_account| match &result_reply {
        //                 GrpcCallReplyKind::GarbleStandard(reply) => {
        //                     Call::callback_new_garbled_signed {
        //                         pgarbled_cid: reply.pgarbled_cid.bytes().collect(),
        //                     }
        //                 }
        //                 GrpcCallReplyKind::GarbleAndStrip(
        //                     message_reply,
        //                     message_digits,
        //                     pinpad_reply,
        //                     pinpad_digits,
        //                 ) => Call::callback_new_garbled_and_strip_signed {
        //                     message_pgarbled_cid: message_reply.pgarbled_cid.bytes().collect(),
        //                     message_packmsg_cid: message_reply.packmsg_cid.bytes().collect(),
        //                     message_digits: message_digits.to_vec(),
        //                     pinpad_pgarbled_cid: pinpad_reply.pgarbled_cid.bytes().collect(),
        //                     pinpad_packmsg_cid: pinpad_reply.packmsg_cid.bytes().collect(),
        //                     pinpad_digits: pinpad_digits.to_vec(),
        //                 },
        //             });
        //             log::info!(
        //                 "[ocw-garble] finalize_grpc_call sent number : {:#?}",
        //                 results.len()
        //             );
        //         }
        //         Err(err) => {
        //             log::error!("[ocw-garble] finalize_grpc_call: error: {:?}", err);
        //         }
        //     }
        // }
        */
    }

    // needed for with_block_and_time_deadline()
    impl<T: Config> BlockNumberProvider for Pallet<T> {
        type BlockNumber = T::BlockNumber;

        fn current_block_number() -> Self::BlockNumber {
            <frame_system::Pallet<T>>::block_number()
        }
    }

    fn get_ipfs_uri() -> alloc::string::String {
        #[cfg(all(not(feature = "sgx"), feature = "std"))]
        return std::env::var("IPFS_ROOT_URL").unwrap();

        #[cfg(all(not(feature = "std"), feature = "sgx"))]
        return sgx_tstd::env::var("IPFS_ROOT_URL").unwrap();
    }

    fn get_node_uri() -> alloc::string::String {
        #[cfg(all(not(feature = "sgx"), feature = "std"))]
        return std::env::var("INTERSTELLAR_URI_NODE").unwrap();

        #[cfg(all(not(feature = "std"), feature = "sgx"))]
        return sgx_tstd::env::var("INTERSTELLAR_URI_NODE").unwrap();
    }

    /// Compute the Storage key; version for RPC
    /// ie it is `compute_storage_hash` but hex encoded
    ///
    /// cf https://docs.substrate.io/build/remote-procedure-calls/
    /// NOTE: this is bad, it will fail if the storage name changes, and the compiler will not catch it!
    /// https://substrate.stackexchange.com/questions/3354/access-storage-map-from-another-pallet-without-trait-pallet-config
    pub(crate) fn compute_storage_hash_hex_for_rpc(pallet: &str, storage_key: &str) -> String {
        let raw_hash =
            frame_support::storage::storage_prefix(pallet.as_bytes(), storage_key.as_bytes());

        "0x".to_string() + &hex::encode(raw_hash)
    }

    /// INTERNAL: call API_ENDPOINT_GARBLE_STRIP_URL for one circuits
    fn call_grpc_garble_and_strip_one<T: Config>(
        skcd_cid: Vec<u8>,
        tx_msg: Vec<u8>,
        digits: Vec<u8>,
    ) -> Result<crate::GarbleAndStripIpfsReply, Error<T>> {
        let skcd_cid_str = sp_std::str::from_utf8(&skcd_cid)
            .map_err(|_err| <Error<T>>::Utf8Error)?
            .to_owned();
        let tx_msg_str = sp_std::str::from_utf8(&tx_msg)
            .map_err(|_err| <Error<T>>::Utf8Error)?
            .to_owned();

        let ipfs_client = interstellar_ipfs_client::IpfsClientHttpReq::new(&get_ipfs_uri())
            .map_err(|err| {
                log::error!("[ocw-garble] ipfs client new error: {:?}", err);
                <Error<T>>::IpfsClientCreationError
            })?;
        let skcd_buf = ipfs_client.ipfs_cat(&skcd_cid_str).map_err(|err| {
            log::error!("[ocw-garble] ipfs call ipfs_cat error: {:?}", err);
            <Error<T>>::IpfsCallError
        })?;

        let garb = lib_garble_rs::garble_skcd(&skcd_buf).map_err(|err| {
            log::error!(
                "[ocw-garble] lib_garble_rs::garble_skcd error: {:?} {:?}",
                err.to_string(),
                err
            );
            <Error<T>>::GarblerError
        })?;
        // "packsmg"
        let encoded_garbler_inputs =
            lib_garble_rs::garbled_display_circuit_prepare_garbler_inputs(
                &garb,
                &digits,
                &tx_msg_str,
            )
            .map_err(|err| {
                log::error!(
                    "[ocw-garble] lib_garble_rs::garbled_display_circuit_prepare_garbler_inputs error: {:?} {:?}",
                    err.to_string(),
                    err
                );
                <Error<T>>::GarblerError
            })?;
        // then serialize "garb" and "packmsg"
        let serialized_package_for_eval =
            lib_garble_rs::serialize_for_evaluator(garb, encoded_garbler_inputs).map_err(
                |err| {
                    log::error!(
                        "[ocw-garble] lib_garble_rs::serialize_for_evaluator error: {:?} {:?}",
                        err.to_string(),
                        err
                    );
                    <Error<T>>::GarblerError
                },
            )?;

        // the tests need the full body bytes to mock correctly...
        #[cfg(test)]
        let serialized_package_for_eval =
            T::HookCallGrpGarbleAndStripSerializedPackageForEval::my_test_hook(
                serialized_package_for_eval,
            );

        let ipfs_add_response =
            ipfs_client
                .ipfs_add(&serialized_package_for_eval)
                .map_err(|err| {
                    log::error!("[ocw-garble] ipfs call ipfs_add error: {:?}", err);
                    <Error<T>>::IpfsCallError
                })?;

        // TODO
        // let resp: GarbleAndStripIpfsReply = ;
        Ok(crate::GarbleAndStripIpfsReply {
            pgarbled_cid: ipfs_add_response.hash,
        })
    }
}
