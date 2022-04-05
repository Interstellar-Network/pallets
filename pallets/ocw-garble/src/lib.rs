#![cfg_attr(not(feature = "std"), no_std)]

use bytes::Buf;
use bytes::BufMut;
// use codec::{Decode, Encode};
use frame_support::traits::Get;
use frame_system::{
    self as system,
    offchain::{
        AppCrypto, CreateSignedTransaction, SendSignedTransaction, SendUnsignedTransaction,
        SignedPayload, Signer, SigningTypes, SubmitTransaction,
    },
};
use sp_core::{crypto::KeyTypeId, hexdisplay::AsBytesRef};
use sp_runtime::{
    offchain::{
        http,
        storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
        Duration,
    },
    traits::Zero,
    transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
    RuntimeDebug,
};
use sp_std::borrow::ToOwned;
use sp_std::vec;
use sp_std::vec::Vec;

use prost::Message;

mod interstellarpbapigarble {
    // include_bytes!(concat!(env!("OUT_DIR")), "/interstellarpbapigarble.rs");
    // include_bytes!(concat!(env!("OUT_DIR"), "/interstellarpbapigarble.rs"));
    include!(concat!(env!("OUT_DIR"), "/interstellarpbapigarble.rs"));
}

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    //! A demonstration of an offchain worker that sends onchain callbacks
    use codec::{Decode, Encode};
    use core::convert::TryInto;
    use frame_support::pallet_prelude::*;
    use frame_system::{
        offchain::{
            AppCrypto, CreateSignedTransaction, SendSignedTransaction, SendUnsignedTransaction,
            SignedPayload, Signer, SigningTypes, SubmitTransaction,
        },
        pallet_prelude::*,
    };
    use sp_core::crypto::KeyTypeId;
    use sp_runtime::{
        offchain::{
            http,
            storage::StorageValueRef,
            storage_lock::{BlockAndTime, StorageLock},
            Duration,
        },
        traits::BlockNumberProvider,
        transaction_validity::{
            InvalidTransaction, TransactionSource, TransactionValidity, ValidTransaction,
        },
        RuntimeDebug,
    };
    use sp_std::{collections::vec_deque::VecDeque, prelude::*, str};

    use serde::{Deserialize, Deserializer};

    /// Defines application identifier for crypto keys of this module.
    ///
    /// Every module that deals with signatures needs to declare its unique identifier for
    /// its crypto keys.
    /// When an offchain worker is signing transactions it's going to request keys from type
    /// `KeyTypeId` via the keystore to sign the transaction.
    /// The keys can be inserted manually via RPC (see `author_insertKey`).
    pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"demo");
    const NUM_VEC_LEN: usize = 10;
    /// The type to sign and send transactions.
    const UNSIGNED_TXS_PRIORITY: u64 = 100;

    const FETCH_TIMEOUT_PERIOD: u64 = 3000; // in milli-seconds
    const LOCK_TIMEOUT_EXPIRATION: u64 = FETCH_TIMEOUT_PERIOD + 1000; // in milli-seconds
    const LOCK_BLOCK_EXPIRATION: u32 = 3; // in block number

    const ONCHAIN_TX_KEY: &[u8] = b"ocw-garble::storage::tx";
    const LOCK_KEY: &[u8] = b"ocw-garble::lock";

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

    #[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, scale_info::TypeInfo)]
    pub struct Payload<Public> {
        skcd_cid: Vec<u8>,
        public: Public,
    }

    impl<T: SigningTypes> SignedPayload<T> for Payload<T::Public> {
        fn public(&self) -> T::Public {
            self.public.clone()
        }
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> {
        /// The overarching event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
        /// The overarching dispatch call type.
        type Call: From<Call<Self>>;
        /// The identifier type for an offchain worker.
        type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        // TODO NewDisplayConfig(Option<T::AccountId>, u32, u32),
        NewSkcdIpfsCid(Option<T::AccountId>, Vec<u8>),
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
        HttpFetchingError,
        DeserializeToObjError,
        DeserializeToStrError,
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
            log::info!("[ocw-garble] Hello from pallet-ocw-garble.");

            let result = Self::fetch_remote_info(block_number);

            if let Err(e) = result {
                log::error!("[ocw-garble] offchain_worker error: {:?}", e);
            }
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        /// Validate unsigned call to this module.
        ///
        /// By default unsigned transactions are disallowed, but implementing the validator
        /// here we make sure that some particular calls (the ones produced by offchain worker)
        /// are being whitelisted and marked as valid.
        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            // TODO?
            InvalidTransaction::Call.into()
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10000)]
        pub fn submit_skcd_cid_signed(origin: OriginFor<T>, skcd_cid: Vec<u8>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            log::info!(
                "[ocw-garble] submit_skcd_cid_signed: ({}, {:?})",
                sp_std::str::from_utf8(&skcd_cid).expect("skcd_cid utf8"),
                who
            );

            let copy = skcd_cid.clone();
            Self::append_or_replace_skcd_hash(skcd_cid);

            Self::deposit_event(Event::NewSkcdIpfsCid(Some(who), copy));
            Ok(())
        }

        #[pallet::weight(10000)]
        pub fn submit_skcd_cid_unsigned(origin: OriginFor<T>, skcd_cid: Vec<u8>) -> DispatchResult {
            let _ = ensure_none(origin)?;
            log::info!(
                "[ocw-garble] submit_skcd_cid_unsigned: {}",
                sp_std::str::from_utf8(&skcd_cid).expect("skcd_cid utf8")
            );

            let copy = skcd_cid.clone();
            Self::append_or_replace_skcd_hash(skcd_cid);

            Self::deposit_event(Event::NewSkcdIpfsCid(None, copy));
            Ok(())
        }

        // #[pallet::weight(10000)]
        // #[allow(unused_variables)]
        // pub fn submit_number_unsigned_with_signed_payload(
        //     origin: OriginFor<T>,
        //     payload: Payload<T::Public>,
        //     signature: T::Signature,
        // ) -> DispatchResult {
        //     let _ = ensure_none(origin)?;
        //     // we don't need to verify the signature here because it has been verified in
        //     //   `validate_unsigned` function when sending out the unsigned tx.
        //     let Payload { skcd_cid, public } = payload;
        //     log::info!(
        //         "[ocw-garble] submit_number_unsigned_with_signed_payload: ({}, {:?})",
        //         skcd_cid,
        //         public
        //     );
        //     Self::append_or_replace_skcd_hash(skcd_cid);

        //     Self::deposit_event(Event::New(None, skcd_cid));
        //     Ok(())
        // }
    }

    impl<T: Config> Pallet<T> {
        fn derived_key() -> Vec<u8> {
            // TODO re-add block_number?
            let block_number = T::BlockNumber::default();
            block_number.using_encoded(|encoded_bn| {
                ONCHAIN_TX_KEY
                    .clone()
                    .into_iter()
                    .chain(b"/".into_iter())
                    .chain(encoded_bn)
                    .copied()
                    .collect::<Vec<u8>>()
            })
        }
    }

    #[derive(Debug, Deserialize, Encode, Decode, Default)]
    struct IndexingData {
        skcd_ipfs_hash: Vec<u8>,
        block_number: u32,
    }

    impl IndexingData {
        fn empty() -> IndexingData {
            IndexingData {
                skcd_ipfs_hash: Vec::<u8>::new(),
                block_number: 0,
            }
        }
    }

    impl<T: Config> Pallet<T> {
        /// Append a new number to the tail of the list, removing an element from the head if reaching
        ///   the bounded length.
        fn append_or_replace_skcd_hash(skcd_cid: Vec<u8>) {
            let key = Self::derived_key();
            let data = IndexingData {
                skcd_ipfs_hash: skcd_cid,
                block_number: 1,
            };
            sp_io::offchain_index::set(&key, &data.encode());
        }

        /// Check if we have fetched the data before. If yes, we can use the cached version
        ///   stored in off-chain worker storage `storage`. If not, we fetch the remote info and
        ///   write the info into the storage for future retrieval.
        fn fetch_remote_info(block_number: T::BlockNumber) -> Result<(), Error<T>> {
            // Reading back the off-chain indexing value. It is exactly the same as reading from
            // ocw local storage.
            //
            // IMPORTANT: writing using eg StorageValue(mutate,set,kill,take) works but DOES NOTHING
            // During the next call, the old value is there!
            // So we MUST use StorageValueRef/LocalStorage to write.
            let key = Self::derived_key();
            let oci_mem = StorageValueRef::persistent(&key);

            let indexing_data = oci_mem
                .get::<IndexingData>()
                .unwrap_or(Some(IndexingData::empty()))
                .unwrap_or(IndexingData::empty());

            let to_process_skcd_cid = indexing_data.skcd_ipfs_hash;
            let to_process_block_number = indexing_data.block_number;

            // TODO proper job queue; or at least proper CHECK
            if to_process_skcd_cid.is_empty() || to_process_block_number == 0 {
                log::info!("[ocw-garble] nothing to do, returning...");
                return Ok(());
            }

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
                /// NOTE: remove the task from the "job queue" wether it worked or not
                /// TODO better? But in this case we should only retry in case of "remote error"
                /// and NOT retry if eg the given hash is not a valid IPFS hash
                ///
                /// DO NOT use "sp_io::offchain_index::set"!
                /// We MUST use "StorageValueRef::persistent" else the value is not updated??
                oci_mem.set(&IndexingData::empty());

                match Self::fetch_n_parse(&to_process_skcd_cid) {
                    Ok(info) => {
                        // TODO return result via tx
                        // s_info.set(&info);
                        log::info!("[ocw-garble] FINAL got result IPFS hash : {:x?}", info);
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            Ok(())
        }

        /// Fetch from remote and deserialize the JSON to a struct
        fn fetch_n_parse(skcd_cid: &Vec<u8>) -> Result<Vec<u8>, Error<T>> {
            let resp_bytes = Self::fetch_from_remote(skcd_cid).map_err(|e| {
                log::error!("[ocw-garble] fetch_from_remote error: {:?}", e);
                <Error<T>>::HttpFetchingError
            })?;

            let resp_str =
                str::from_utf8(&resp_bytes).map_err(|_| <Error<T>>::DeserializeToStrError)?;
            // Print out our fetched JSON string
            log::info!("[ocw-garble] fetch_n_parse: {}", resp_str);

            Ok(resp_str.encode())
        }

        /// This function uses the `offchain::http` API to query the remote endpoint information,
        ///   and returns the JSON response as vector of bytes.
        fn fetch_from_remote(skcd_cid: &Vec<u8>) -> Result<Vec<u8>, http::Error> {
            // We want to keep the offchain worker execution time reasonable, so we set a hard-coded
            // deadline to 2s to complete the external call.
            // You can also wait idefinitely for the response, however you may still get a timeout
            // coming from the host machine.
            let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));

            // TODO get from payload(ie tx)
            let body = crate::encode_body2(skcd_cid);
            log::info!("[ocw-garble] sending body b64: {}", base64::encode(&body));

            // Initiate an external HTTP GET request.
            // This is using high-level wrappers from `sp_runtime`, for the low-level calls that
            // you can find in `sp_io`. The API is trying to be similar to `reqwest`, but
            // since we are running in a custom WASM execution environment we can't simply
            // import the library here.
            //
            // cf https://github.com/hyperium/tonic/blob/master/tonic-web/tests/integration/tests/grpc_web.rs
            // syntax = "proto3";
            // package test;
            // service Test {
            //		rpc SomeRpc(Input) returns (Output);
            // -> curl http://127.0.0.1:3000/test.Test/SomeRpc
            //
            // NOTE application/grpc-web == application/grpc-web+proto
            //      application/grpc-web-text = base64
            //
            // eg:
            // printf '\x00\x00\x00\x00\x05\x08\xe0\x01\x10\x60' | curl -skv -H "Content-Type: application/grpc-web+proto" -H "X-Grpc-Web: 1" -H "Accept: application/grpc-web-text+proto" -X POST --data-binary @- http://127.0.0.1:3000/interstellarpbapigarble.SkcdApi/GenerateSkcdDisplay
            let request = http::Request::post(
                "http://127.0.0.1:3001/interstellarpbapigarble.GarbleApi/GarbleIpfs",
                vec![body],
            )
            .add_header("Content-Type", "application/grpc-web")
            .add_header("X-Grpc-Web", "1");

            // We set the deadline for sending of the request, note that awaiting response can
            // have a separate deadline. Next we send the request, before that it's also possible
            // to alter request headers or stream body content in case of non-GET requests.
            let pending = request
                .deadline(deadline)
                .send()
                .map_err(|_| http::Error::IoError)?;

            // The request is already being processed by the host, we are free to do anything
            // else in the worker (we can send multiple concurrent requests too).
            // At some point however we probably want to check the response though,
            // so we can block current thread and wait for it to finish.
            // Note that since the request is being driven by the host, we don't have to wait
            // for the request to have it complete, we will just not read the response.
            let mut response = pending
                .try_wait(deadline)
                .map_err(|_| http::Error::DeadlineReached)??;

            log::info!("[ocw-garble] status code: {}", response.code);
            let mut headers_it = response.headers().into_iter();
            while headers_it.next() {
                let header = headers_it.current().unwrap();
                log::info!("[ocw-garble] header: {} {}", header.0, header.1);
            }

            // Let's check the status code before we proceed to reading the response.
            if response.code != 200 {
                log::warn!("[ocw-garble] Unexpected status code: {}", response.code);
                return Err(http::Error::Unknown);
            }

            // TODO handle like parse_price
            let body_bytes = response.body().collect::<bytes::Bytes>();
            let (reply, trailers) = crate::decode_body2(body_bytes, "application/grpc-web");

            log::info!(
                "[ocw-garble] Got gRPC trailers: {}",
                sp_std::str::from_utf8(&trailers).expect("trailers")
            );
            log::info!("[ocw-garble] Got IPFS hash: {}", reply.pgarbled_cid);

            Ok(reply.pgarbled_cid.bytes().collect())
        }
    }

    impl<T: Config> BlockNumberProvider for Pallet<T> {
        type BlockNumber = T::BlockNumber;

        fn current_block_number() -> Self::BlockNumber {
            <frame_system::Pallet<T>>::block_number()
        }
    }
}

// we CAN NOT just send the raw encoded protobuf(eg using GarbleIpfsRequest{}.encode())
// b/c that returns errors like
// "protocol error: received message with invalid compression flag: 8 (valid flags are 0 and 1), while sending request"
// "tonic-web: Invalid byte 45, offset 0"
// https://github.com/hyperium/tonic/blob/01e5be508051eebf19c233d48b57797a17331383/tonic-web/tests/integration/tests/grpc_web.rs#L93
// also: https://github.com/grpc/grpc-web/issues/152
fn encode_body2(skcd_cid: &Vec<u8>) -> bytes::Bytes {
    log::info!("[ocw-garble] encode_body2: {:x?}", skcd_cid);

    let skcd_cid_str = sp_std::str::from_utf8(&skcd_cid)
        .expect("encode_body2 from_utf8")
        .to_owned();
    let input = interstellarpbapigarble::GarbleIpfsRequest {
        skcd_cid: skcd_cid_str,
    };

    let mut buf = bytes::BytesMut::with_capacity(1024);
    buf.reserve(5);
    unsafe {
        buf.advance_mut(5);
    }

    input.encode(&mut buf).unwrap();

    let len = buf.len() - 5;
    {
        let mut buf = &mut buf[..5];
        buf.put_u8(0);
        buf.put_u32(len as u32);
    }

    buf.split_to(len + 5).freeze()
}

fn decode_body2(
    body_bytes: bytes::Bytes,
    content_type: &str,
) -> (interstellarpbapigarble::GarbleIpfsReply, bytes::Bytes) {
    let mut body = body_bytes;

    if content_type == "application/grpc-web-text+proto" {
        body = base64::decode(body).unwrap().into()
    }

    body.advance(1);
    let len = body.get_u32();
    let msg = interstellarpbapigarble::GarbleIpfsReply::decode(&mut body.split_to(len as usize))
        .expect("decode");
    body.advance(5);

    (msg, body)
}
