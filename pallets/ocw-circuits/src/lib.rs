#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

extern crate alloc;

use frame_system::offchain::AppCrypto;
use frame_system::offchain::CreateSignedTransaction;
use scale_info::prelude::*;
use sp_core::crypto::KeyTypeId;
use sp_runtime::traits::BlockNumberProvider;
use sp_runtime::transaction_validity::InvalidTransaction;
use sp_std::prelude::*;
use sp_std::str;
use sp_std::vec::Vec;

// NOTE: "cf MUST NOT try to compile "lib_circuits" for WASM" in Cargo.toml
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use alloc::string::String;
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use alloc::string::ToString;
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use frame_system::ensure_signed;
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use frame_system::offchain::{SendSignedTransaction, Signer};
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use interstellar_ipfs_client::IpfsClient;
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use sp_runtime::offchain::{
    storage::StorageValueRef,
    storage_lock::{BlockAndTime, StorageLock},
    Duration,
};
#[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
use sp_std::borrow::ToOwned;

pub use pallet::*;

/// TEST ONLY "hook"
/// Because the tests need the full body bytes to mock correctly...
/// It is used to overwrite the result of `lib_circuits_rs::serialize`
///
/// cf https://github.com/paritytech/substrate/pull/12307/files
///
#[cfg(test)]
pub trait MyTestCallback {
    fn my_test_hook(input: Vec<u8>) -> Vec<u8> {
        input
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use circuits_storage_common::DisplaySkcdPackage;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    /// Defines application identifier for crypto keys of this module.
    ///
    /// Every module that deals with signatures needs to declare its unique identifier for
    /// its crypto keys.
    /// When an offchain worker is signing transactions it's going to request keys from type
    /// `KeyTypeId` via the keystore to sign the transaction.
    /// The keys can be inserted manually via RPC (see `author_insertKey`).
    pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"circ");

    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const LOCK_TIMEOUT_EXPIRATION: u64 = 10000; // in milli-seconds
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const LOCK_BLOCK_EXPIRATION: u32 = 3; // in block number

    const ONCHAIN_TX_KEY: &[u8] = b"ocw-circuits::storage::tx";
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const LOCK_KEY: &[u8] = b"ocw-circuits::lock";

    // Resolutions for the "message" mode and the "pinpad" mode
    // There are no good/bad ones, it is only trial and error.
    // You SHOULD use lib_circuits's cli_display_skcd to try and find good ones.
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const DEFAULT_MESSAGE_WIDTH: u32 = 1280 / 2;
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const DEFAULT_MESSAGE_HEIGHT: u32 = 720 / 2;
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const DEFAULT_PINPAD_WIDTH: u32 = 590;
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    const DEFAULT_PINPAD_HEIGHT: u32 = 50;

    /// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrapper.
    /// We can utilize the supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
    /// them with the pallet-specific identifier.
    pub mod crypto {
        use crate::KEY_TYPE;
        use alloc::format;
        use alloc::string::String;
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

    #[pallet::config]
    pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> + 'static {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// The overarching dispatch call type.
        type RuntimeCall: From<Call<Self>>;
        /// A dispatchable call.
        // type RuntimeCall: Parameter
        //     + Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
        //     + GetDispatchInfo
        //     + From<frame_system::Call<Self>>;
        /// The identifier type for an offchain worker.
        type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
        #[cfg(test)]
        type HookCallPostSerializedPackage: MyTestCallback;
    }

    /// For now it will be stored as a StorageValue but later we could use
    /// a map for various resolutions, kind of digits(7 segments vs other?), etc
    #[pallet::storage]
    pub(super) type DisplaySkcdPackageValue<T: Config> =
        StorageValue<_, DisplaySkcdPackage, ValueQuery>;

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
        NewSkcdIpfsCid(Vec<u8>),
        // Display version: one IPFS cid for the message, one IPFS cid for the pinpad
        NewDisplaySkcdPackageIpfsCid(Vec<u8>, Vec<u8>),
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {
        // get_display_circuits_package(ie pallet_ocw_garble) was called
        // but "DisplaySkcdPackageValue" is not completely set
        DisplaySkcdPackageValueError,
        IpfsClientCreationError,
        IpfsCallError,

        CircuitDisplayGenerateError,
        CircuitSerializeError,
        CircuitGenericGenerateError,

        StorageGetError,
        /// Special case: not really an error
        OffchainNothingToDoWarning,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
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
            log::info!("[ocw-circuits] Hello from pallet-ocw-circuits.");

            // TODO proper job queue; eg use last_run_block_number and process all the needed ones
            #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
            let result = Self::process_if_needed(block_number);

            #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
            if let Err(e) = result {
                log::error!("[ocw-circuits] offchain_worker error: {:?}", e);
            }

            #[cfg(not(all(feature = "circuit-gen-rs", not(target_family = "wasm"))))]
            log::info!(
                "[ocw-circuits] Running without feature `circuit-gen-rs; no-op! {:?}`",
                block_number
            );
        }
    }

    /// Return the stored `DisplaySkcdPackageValue` if it exists, else Error!
    ///
    /// This is called by `pallet-ocw-garble`!
    ///
    pub fn get_display_circuits_package<T: Config>() -> Result<DisplaySkcdPackage, Error<T>> {
        let display_circuit_package = <DisplaySkcdPackageValue<T>>::get();

        // CHECK: error-out if both fields are not set
        if display_circuit_package.message_skcd_server_metadata_nb_digits == 0
            || display_circuit_package.pinpad_skcd_server_metadata_nb_digits == 0
        {
            return Err(<Error<T>>::DisplaySkcdPackageValueError);
        }

        Ok(display_circuit_package)
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

    // cf https://substrate.stackexchange.com/questions/7804/how-to-conditionally-compile-functions-in-palletcall
    // for why we need an indirection...
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(10000)]
        pub fn submit_config_generic_signed(
            origin: OriginFor<T>,
            verilog_cid: Vec<u8>,
        ) -> DispatchResult {
            Self::submit_config_generic_signed_impl(origin, verilog_cid)
        }

        #[pallet::call_index(1)]
        #[pallet::weight(10000)]
        pub fn submit_config_display_circuits_package_signed(
            origin: OriginFor<T>,
        ) -> DispatchResult {
            Self::submit_config_display_circuits_package_signed_impl(origin)
        }

        /// Called at the end of offchain_worker to publish the result
        /// Not meant to be called by a user
        // TODO use "with signed payload" and check if expected key?
        #[pallet::call_index(2)]
        #[pallet::weight(10000)]
        pub fn callback_new_skcd_signed(origin: OriginFor<T>, skcd_cid: Vec<u8>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            log::info!(
                "[ocw-circuits] callback_new_skcd_signed: {:?} for {:?})",
                sp_std::str::from_utf8(&skcd_cid).expect("skcd_cid utf8"),
                who
            );

            Self::deposit_event(Event::NewSkcdIpfsCid(skcd_cid));
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(10000)]
        pub fn callback_new_display_circuits_package_signed(
            origin: OriginFor<T>,
            message_skcd_cid: Vec<u8>,
            message_nb_digits: u32,
            pinpad_skcd_cid: Vec<u8>,
            pinpad_nb_digits: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            log::info!(
                "[ocw-circuits] callback_new_display_circuits_package_signed: ({:?},{:?}),({:?},{:?}) for {:?}",
                sp_std::str::from_utf8(&message_skcd_cid).expect("message_skcd_cid utf8"),
                message_nb_digits,
                sp_std::str::from_utf8(&pinpad_skcd_cid).expect("pinpad_skcd_cid utf8"),
                pinpad_nb_digits,
                who
            );

            Self::deposit_event(Event::NewDisplaySkcdPackageIpfsCid(
                message_skcd_cid.clone(),
                pinpad_skcd_cid.clone(),
            ));

            // and update the current "reference" circuits package
            <DisplaySkcdPackageValue<T>>::set(DisplaySkcdPackage {
                message_skcd_cid: TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(
                    message_skcd_cid,
                )
                .unwrap(),
                message_skcd_server_metadata_nb_digits: message_nb_digits,
                pinpad_skcd_cid: TryInto::<BoundedVec<u8, ConstU32<64>>>::try_into(pinpad_skcd_cid)
                    .unwrap(),
                pinpad_skcd_server_metadata_nb_digits: pinpad_nb_digits,
            });

            Ok(())
        }
    }

    #[derive(Debug, Encode, Decode, Default)]
    pub(crate) enum GrpcCallKind {
        Generic {
            verilog_cid: Vec<u8>,
        },
        #[default]
        Display,
    }

    /// Results wrappers for the calls to "lib_circuits_rs::"
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    enum LibCircuitsRsResultKind {
        Generic {
            ipfs_hash: String,
        },
        // one reply for message, one for pinpad
        Display {
            message_ipfs_hash: String,
            message_nb_digits: u32,
            pinpad_ipfs_hash: String,
            pinpad_nb_digits: u32,
        },
    }

    #[derive(Debug, Encode, Decode, Default)]
    pub(crate) struct IndexingData {
        // // verilog_ipfs_hash only if GrpcCallKind::Generic
        // // (For now) when it is GrpcCallKind::Display the corresponding Verilog are packaged in the repo api_circuits
        // // = in "display mode" the Verilog are hardcoded, NOT passed dynamically via IPFS; contrary to "generic mode"
        // verilog_ipfs_hash: Option<Vec<u8>>,
        pub(crate) grpc_kind: GrpcCallKind,
    }

    impl<T: Config> Pallet<T> {
        pub fn derived_key() -> Vec<u8> {
            // TODO re-add block_number?
            let block_number = T::BlockNumber::default();
            block_number.using_encoded(|encoded_bn| {
                ONCHAIN_TX_KEY
                    .iter()
                    .chain(b"/".iter())
                    .chain(encoded_bn)
                    .copied()
                    .collect::<Vec<u8>>()
            })
        }

        /// Append a new number to the tail of the list, removing an element from the head if reaching
        ///   the bounded length.
        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn add_job_to_queue(grpc_kind: GrpcCallKind) {
            let key = Self::derived_key();
            let data = IndexingData { grpc_kind };
            sp_io::offchain_index::set(&key, &data.encode());
        }

        /// Check if we have fetched the data before. If yes, we can use the cached version
        ///   stored in off-chain worker storage `storage`. If not, we fetch the remote info and
        ///   write the info into the storage for future retrieval.
        ///
        /// https://github.com/JoshOrndorff/recipes/blob/master/text/off-chain-workers/storage.md
        /// https://gist.github.com/spencerbh/1a150e076f4cef0ff4558642c4837050
        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn process_if_needed(_block_number: T::BlockNumber) -> Result<(), Error<T>> {
            // Reading back the off-chain indexing value. It is exactly the same as reading from
            // ocw local storage.
            //
            // IMPORTANT: writing using eg StorageValue(mutate,set,kill,take) works but DOES NOTHING
            // During the next call, the old value is there!
            // So we MUST use StorageValueRef/LocalStorage to write.
            let key = Self::derived_key();
            let mut oci_mem = StorageValueRef::persistent(&key);

            let indexing_data = oci_mem
                .get::<IndexingData>()
                .map_err(|err| {
                    log::warn!("[ocw-circuits] StorageRetrievalError... : {err:?}");
                    <Error<T>>::StorageGetError
                })?
                .ok_or_else(|| {
                    log::info!("[ocw-circuits] nothing to do, returning...");
                    <Error<T>>::OffchainNothingToDoWarning
                })?;

            // Since off-chain storage can be accessed by off-chain workers from multiple runs, it is important to lock
            //   it before doing heavy computations or write operations.
            //
            // There are four ways of defining a lock:
            //   1) `new` - lock with default time and block exipration
            //   2) `with_deadline` - lock with default block but custom time expiration
            //   3) `with_block_deadline` - lock with default time but custom block expiration
            //   4) `with_block_and_time_deadline` - lock with custom time and block expiration
            // Here we choose the most custom one for demonstration purpose.
            let mut lock = StorageLock::<BlockAndTime<Self>>::with_block_and_time_deadline(
                LOCK_KEY,
                LOCK_BLOCK_EXPIRATION,
                Duration::from_millis(LOCK_TIMEOUT_EXPIRATION),
            );

            // We try to acquire the lock here. If failed, we know the `fetch_n_parse` part inside is being
            //   executed by previous run of ocw, so the function just returns.
            if let Ok(_guard) = lock.try_lock() {
                // NOTE: remove the task from the "job queue" wether it worked or not
                // TODO better? But in this case we should only retry in case of "remote error"
                // and NOT retry if eg the given hash is not a valid IPFS hash
                //
                // DO NOT use "sp_io::offchain_index::set"!
                // We MUST use "StorageValueRef::persistent" else the value is not updated??
                oci_mem.clear();

                let result_grpc_call = match indexing_data.grpc_kind {
                    GrpcCallKind::Generic { verilog_cid } => Self::call_grpc_generic(&verilog_cid)?,
                    GrpcCallKind::Display => Self::call_grpc_display()?,
                };

                Self::finalize_grpc_call(result_grpc_call);
            }

            Ok(())
        }

        #[cfg(not(all(feature = "circuit-gen-rs", not(target_family = "wasm"))))]
        fn submit_config_generic_signed_impl(
            _origin: OriginFor<T>,
            _verilog_cid: Vec<u8>,
        ) -> DispatchResult {
            unimplemented!("submit_config_generic_signed_impl: require feature circuit-gen-rs")
        }

        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn submit_config_generic_signed_impl(
            origin: OriginFor<T>,
            verilog_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            log::info!(
                "[ocw-circuits] submit_config_generic_signed: ({:?}, {:?})",
                sp_std::str::from_utf8(&verilog_cid).expect("verilog_cid utf8"),
                who
            );

            Self::add_job_to_queue(GrpcCallKind::Generic { verilog_cid });

            Ok(())
        }

        #[cfg(not(all(feature = "circuit-gen-rs", not(target_family = "wasm"))))]
        fn submit_config_display_circuits_package_signed_impl(
            _origin: OriginFor<T>,
        ) -> DispatchResult {
            unimplemented!("submit_config_display_circuits_package_signed_impl: require feature circuit-gen-rs")
        }

        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn submit_config_display_circuits_package_signed_impl(
            origin: OriginFor<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            log::info!(
                "[ocw-circuits] submit_config_display_circuits_package_signed: ({:?})",
                who
            );

            Self::add_job_to_queue(GrpcCallKind::Display);

            Ok(())
        }

        /// Call the GRPC endpoint API_ENDPOINT_GENERIC_URL, encoding the request as grpc-web, and decoding the response
        ///
        /// return: a IPFS hash
        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn call_grpc_generic(verilog_cid: &[u8]) -> Result<LibCircuitsRsResultKind, Error<T>> {
            let ipfs_client = interstellar_ipfs_client::IpfsClientSpOffchain::new(&get_ipfs_uri())
                .map_err(|err| {
                    log::error!("[ocw-circuits] ipfs client new error: {:?}", err);
                    <Error<T>>::IpfsClientCreationError
                })?;

            let verilog_cid_str = sp_std::str::from_utf8(verilog_cid)
                .expect("call_grpc_generic from_utf8")
                .to_owned();

            // TODO(lib_circuits) get .v from IPFS (eg use ipfs_cat), and use new eg `lib_circuits_rs::new_from_verilog`
            let verilog_buf = ipfs_client.ipfs_cat(&verilog_cid_str).map_err(|err| {
                log::error!("[ocw-circuits] ipfs call ipfs_add error: {:?}", err);
                <Error<T>>::IpfsCallError
            })?;

            let circuit = lib_circuits_rs::new_from_verilog(&verilog_buf).map_err(|err| {
                log::error!(
                    "[ocw-circuits] lib_circuits_rs::new_from_verilog error: {:?} {:?}",
                    err.to_string(),
                    err
                );
                <Error<T>>::CircuitGenericGenerateError
            })?;

            // then serialize "garb" and "packmsg"
            let serialized_circuit = lib_circuits_rs::serialize(&circuit).map_err(|err| {
                log::error!(
                    "[ocw-circuits] lib_circuits_rs::serialize error: {:?} {:?}",
                    err.to_string(),
                    err
                );
                <Error<T>>::CircuitSerializeError
            })?;

            // the tests need the full body bytes to mock correctly...
            #[cfg(test)]
            let serialized_circuit =
                T::HookCallPostSerializedPackage::my_test_hook(serialized_circuit);

            let ipfs_add_response = ipfs_client.ipfs_add(&serialized_circuit).map_err(|err| {
                log::error!("[ocw-circuits] ipfs call ipfs_add error: {:?}", err);
                <Error<T>>::IpfsCallError
            })?;

            Ok(LibCircuitsRsResultKind::Generic {
                ipfs_hash: ipfs_add_response.hash,
            })
        }

        /// Call the GRPC endpoint API_ENDPOINT_GENERIC_URL, encoding the request as grpc-web, and decoding the response
        ///
        /// return:
        /// - a IPFS hash
        /// - the number of digits(that was sent to `api_circuits` in the Request, and SHOULD be "burned in" the Garbled Circuit)
        ///   NOTE: it is CRITICAL to expose this number of digits(eg via the Storage, or RPC) b/c `pallet-ocw-garble`
        ///         MUST know it when attempting to garble the circuit to generate the correct number of random digits.
        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn call_grpc_display() -> Result<LibCircuitsRsResultKind, Error<T>> {
            let (message_ipfs_hash, message_nb_digits) = call_grpc_display_one::<T>(true)?;
            let (pinpad_ipfs_hash, pinpad_nb_digits) = call_grpc_display_one::<T>(false)?;

            // TODO pass correct params for pinpad and message
            Ok(LibCircuitsRsResultKind::Display {
                message_ipfs_hash,
                message_nb_digits,
                pinpad_ipfs_hash,
                pinpad_nb_digits,
            })
        }

        /// Called at the end of process_if_needed/offchain_worker
        /// Publish the result back via send_signed_transaction(and Event)
        ///
        /// param: result_grpc_call: returned by call_grpc_display/call_grpc_generic
        #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
        fn finalize_grpc_call(lib_circuits_rs_result: LibCircuitsRsResultKind) {
            // Using `send_signed_transaction` associated type we create and submit a transaction
            // representing the call we've just created.
            // `send_signed_transaction()` return type is `Option<(Account<T>, Result<(), ()>)>`. It is:
            //   - `None`: no account is available for sending transaction
            //   - `Some((account, Ok(())))`: transaction is successfully sent
            //   - `Some((account, Err(())))`: error occurred when sending the transaction
            let signer = Signer::<T, T::AuthorityId>::all_accounts();
            if !signer.can_sign() {
                log::error!(
                    "[ocw-circuits] No local accounts available. Consider adding one via `author_insertKey` RPC[ALTERNATIVE DEV ONLY check 'if config.offchain_worker.enabled' in service.rs]"
                );
            }

            let tx_results =
                signer.send_signed_transaction(|_account| match &lib_circuits_rs_result {
                    LibCircuitsRsResultKind::Generic { ipfs_hash } => {
                        Call::callback_new_skcd_signed {
                            skcd_cid: ipfs_hash.bytes().collect(),
                        }
                    }
                    LibCircuitsRsResultKind::Display {
                        message_ipfs_hash,
                        message_nb_digits,
                        pinpad_ipfs_hash,
                        pinpad_nb_digits,
                    } => Call::callback_new_display_circuits_package_signed {
                        message_skcd_cid: message_ipfs_hash.bytes().collect(),
                        message_nb_digits: *message_nb_digits,
                        pinpad_skcd_cid: pinpad_ipfs_hash.bytes().collect(),
                        pinpad_nb_digits: *pinpad_nb_digits,
                    },
                });

            log::info!(
                "[ocw-circuits] callback_new_skcd_signed sent number : {:#?}",
                tx_results.len()
            );
        }
    }

    // needed for with_block_and_time_deadline()
    impl<T: Config> BlockNumberProvider for Pallet<T> {
        type BlockNumber = T::BlockNumber;

        fn current_block_number() -> Self::BlockNumber {
            <frame_system::Pallet<T>>::block_number()
        }
    }

    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    fn get_ipfs_uri() -> String {
        #[cfg(all(not(feature = "sgx"), feature = "std"))]
        return std::env::var("IPFS_ROOT_URL").unwrap();

        #[cfg(all(not(feature = "std"), feature = "sgx"))]
        return sgx_tstd::env::var("IPFS_ROOT_URL").unwrap();
    }

    /// aux function: call API_ENDPOINT_DISPLAY_URL for either is_message or not
    ///
    /// return:
    /// - IPFS hash
    /// - number of digits
    #[cfg(all(feature = "circuit-gen-rs", not(target_family = "wasm")))]
    fn call_grpc_display_one<T: Config>(is_message: bool) -> Result<(String, u32), Error<T>> {
        let ipfs_client = interstellar_ipfs_client::IpfsClientSpOffchain::new(&get_ipfs_uri())
            .map_err(|err| {
                log::error!("[ocw-circuits] ipfs client new error: {:?}", err);
                <Error<T>>::IpfsClientCreationError
            })?;

        let (width, height, digits_bboxes) = if is_message {
            (
                DEFAULT_MESSAGE_WIDTH,
                DEFAULT_MESSAGE_HEIGHT,
                vec![
                    // first digit bbox --------------------------------------------
                    0.25_f32, 0.1_f32, 0.45_f32, 0.9_f32,
                    // second digit bbox -------------------------------------------
                    0.55_f32, 0.1_f32, 0.75_f32, 0.9_f32,
                ],
            )
        } else {
            // IMPORTANT: by convention the "pinpad" is 10 digits, placed horizontally(side by side)
            // DO NOT change the layout, else wallet-app will NOT display the pinpad correctly!
            // That is b/c this layout in treated as a "texture atlas" so the positions MUST be known.
            // Ideally the positions SHOULD be passed from here all the way into the serialized .pgarbled/.packmsg
            // but this NOT YET the case.

            // 10 digits, 4 corners(vertices) per digit
            let mut digits_bboxes: Vec<f32> = Vec::with_capacity(10 * 4);
            /*
            for (int i = 0; i < 10; i++) {
                digits_bboxes.emplace_back(0.1f * i, 0.0f, 0.1f * (i + 1), 1.0f);
            }
            */
            for i in 0..10 {
                digits_bboxes.append(
                    vec![
                        0.1_f32 * i as f32,
                        0.0_f32,
                        0.1_f32 * (i + 1) as f32,
                        1.0_f32,
                    ]
                    .as_mut(),
                );
            }

            (DEFAULT_PINPAD_WIDTH, DEFAULT_PINPAD_HEIGHT, digits_bboxes)
        };

        let circuit = lib_circuits_rs::generate_display_circuit(width, height, &digits_bboxes)
            .map_err(|err| {
                log::error!(
                    "[ocw-circuits] lib_circuits_rs::generate_display_circuit error: {:?} {:?}",
                    err.to_string(),
                    err
                );
                <Error<T>>::CircuitDisplayGenerateError
            })?;

        // then serialize "garb" and "packmsg"
        let serialized_circuit = lib_circuits_rs::serialize(&circuit).map_err(|err| {
            log::error!(
                "[ocw-circuits] lib_circuits_rs::serialize error: {:?} {:?}",
                err.to_string(),
                err
            );
            <Error<T>>::CircuitSerializeError
        })?;

        // the tests need the full body bytes to mock correctly...
        #[cfg(test)]
        let serialized_circuit = T::HookCallPostSerializedPackage::my_test_hook(serialized_circuit);

        let ipfs_add_response = ipfs_client.ipfs_add(&serialized_circuit).map_err(|err| {
            log::error!("[ocw-circuits] ipfs call ipfs_add error: {:?}", err);
            <Error<T>>::IpfsCallError
        })?;

        // nb_digits: we send in the Request one "BBox" per digit(ie 4 floats)
        // NOTE: if we are here we can guarantee the C++ has checked it was indeed valid BBox so we can / 4 and this is it
        Ok((
            ipfs_add_response.hash,
            (digits_bboxes.len() / 4).try_into().unwrap(),
        ))
    }
}
