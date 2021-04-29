// Offchain worker for DDC metrics.
//
// Inspired from https://github.com/paritytech/substrate/tree/master/frame/example-offchain-worker

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
mod tests;

use frame_support::traits::Get;
use frame_system::{
	offchain::{
		AppCrypto, SendSignedTransaction,
		SigningTypes, Signer, CreateSignedTransaction
	}
};
use frame_support::{
	debug, decl_module, decl_storage, decl_event,
};
use sp_core::crypto::KeyTypeId;
use sp_runtime::{
	offchain::{http, Duration},
	traits::StaticLookup,
};
use codec::{Encode, Decode};
use sp_std::{vec::Vec, str::from_utf8};
use pallet_contracts;

// use lite_json::json::JsonValue;
use alt_serde::{Deserialize, Deserializer};
use hex_literal::hex;


// The address of the metrics contract, in SS58 and in bytes formats.
// To change the address, see tests/mod.rs decode_contract_address().
pub const METRICS_CONTRACT_ADDR: &str = "5Ch5xtvoFF3Muu91WkHCY4mhTDTCyYS2TmBL1zKiBXrYbiZv";
pub const METRICS_CONTRACT_ID: [u8; 32] = [27, 191, 65, 45, 0, 189, 12, 234, 31, 196, 9, 143, 196, 27, 157, 170, 92, 57, 127, 122, 70, 152, 19, 223, 235, 21, 170, 26, 249, 130, 98, 114];

pub const REPORT_METRICS_SELECTOR: [u8; 4] = hex!("35320bbe");

// use serde_json::{Value};

// Specifying serde path as `alt_serde`
// ref: https://serde.rs/container-attrs.html#crate
#[serde(crate = "alt_serde")]
#[derive(Deserialize, Encode, Decode, Default)]
#[derive(Debug)]
#[allow(non_snake_case)]
struct NodeInfo {
	#[serde(deserialize_with = "de_string_to_bytes")]
	httpAddr: Vec<u8>,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
	where
		D: Deserializer<'de>,
{
	let s: &str = Deserialize::deserialize(de)?;
	Ok(s.as_bytes().to_vec())
}


/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ddc1");

pub const HTTP_NODES: &str = "/api/rest/nodes";
pub const HTTP_METRICS: &str = "/api/rest/metrics";
pub const FETCH_TIMEOUT_PERIOD: u64 = 3000; // in milli-seconds

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrappers.
/// We can use from supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// the types with this pallet-specific identifier.
pub mod crypto {
	use super::KEY_TYPE;
	use sp_runtime::{
		app_crypto::{app_crypto, sr25519},
		traits::Verify,
	};
	use frame_system::offchain::AppCrypto;
	use sp_core::sr25519::Signature as Sr25519Signature;
	app_crypto!(sr25519, KEY_TYPE);

	use sp_runtime::{
		MultiSignature,
		MultiSigner,
	};

	pub struct TestAuthId;
	impl AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}

	impl AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}



decl_module! {
	/// A public part of the pallet.
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		//fn deposit_event() = default;

		/// Offchain Worker entry point.
		///
		/// By implementing `fn offchain_worker` within `decl_module!` you declare a new offchain
		/// worker.
		/// This function will be called when the node is fully synced and a new best block is
		/// succesfuly imported.
		/// Note that it's not guaranteed for offchain workers to run on EVERY block, there might
		/// be cases where some blocks are skipped, or for some the worker runs twice (re-orgs),
		/// so the code should be able to handle that.
		/// You can use `Local Storage` API to coordinate runs of the worker.
		/// You can use `debug::native` namespace to not log in wasm mode.
		fn offchain_worker(block_number: T::BlockNumber) {
			debug::info!("[OCW] Hello World from offchain workers!");

			let res = Self::fetch_ddc_data_and_send_to_sc(block_number);

			// TODO: abort on error.
			if let Err(e) = res {
				debug::error!("Some error occurred: {:?}", e);
			}

			debug::info!("[OCW] About to send mock transaction");
			let sc_res = Self::send_to_sc_mock();

			if let Err(e) = sc_res {
				debug::error!("Some error occurred: {:?}", e);
			}

            debug::info!("[OCW] Finishing!");
		}
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as ExampleOffchainWorker {
	}
}

impl<T: Trait> Module<T> {

	fn send_to_sc_mock() -> Result<(), &'static str> {

		debug::info!("[OCW] Getting signer");
		let signer = Signer::<T::CST, T::AuthorityId>::any_account();
		debug::info!("[OCW] Checking signer");
		if !signer.can_sign() {
			return Err(
				"No local accounts available. Consider adding one via `author_insertKey` RPC."
			)?
		}

		debug::info!("[OCW] Continue, signer exists!");

		// Using `send_signed_transaction` associated type we create and submit a transaction
		// representing the call, we've just created.
		// Submit signed will return a vector of results for all accounts that were found in the
		// local keystore with expected `KEY_TYPE`.
		let results = signer.send_signed_transaction(
			|_account| {
				let mut call_data = REPORT_METRICS_SELECTOR.to_vec();
				call_data.extend_from_slice(&[
					1,
					2,
					3,
					4,
				]);

				pallet_contracts::Call::call(
					T::ContractId::get(),
					0u32.into(),
					10_000_000_000,
					call_data
				)
			}
		);

		for (_acc, res) in &results {
			match res {
				Ok(()) => debug::info!("Submitted TX to SC!"),
				Err(e) => debug::error!("Some error occured: {:?}", e),
			}
		}

		Ok(())
	}


	fn fetch_ddc_data_and_send_to_sc(_block_number: T::BlockNumber) -> Result<(), http::Error> {
		let topology = Self::fetch_ddc_network_topology().map_err(|_| "Failed to fetch topology");

		debug::info!("Network topology: {:?}", topology);

		Ok(())
	}

	fn fetch_from_node(one_node: &NodeInfo) -> Result<Vec<u8>, http::Error> {
		let mut node_url = one_node.httpAddr.clone();
		node_url.extend_from_slice(HTTP_METRICS.as_bytes());
		let node_url = from_utf8(&node_url).unwrap();
	
		debug::info!("sending request to: {:?}", node_url);

		// Initiate an external HTTP GET request. This is using high-level wrappers from `sp_runtime`.
		let request = http::Request::get(&node_url);

		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(FETCH_TIMEOUT_PERIOD));

		let pending = request
			.deadline(deadline)
			.send()
			.map_err(|_| http::Error::IoError)?;

		let response = pending.try_wait(deadline)
			.map_err(|_| http::Error::DeadlineReached)??;

		if response.code != 200 {
			debug::warn!("Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown);
		}

		// Next we fully read the response body and collect it to a vector of bytes.
		Ok(response.body().collect::<Vec<u8>>())
	}

	fn fetch_ddc_network_topology() -> Result<(), http::Error> {
		let mut url = T::DdcUrl::get();
		url.extend_from_slice(HTTP_NODES.as_bytes());
		let url = from_utf8(&url).unwrap();

		let request = http::Request::get(&url);

		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(5_000));

		let pending = request
			.deadline(deadline)
			.send()
			.map_err(|_| http::Error::IoError)?;

		let response = pending.try_wait(deadline)
			.map_err(|_| http::Error::DeadlineReached)??;

		if response.code != 200 {
			debug::warn!("Unexpected status code: {}", response.code);
			return Err(http::Error::Unknown);
		}

		let body = response.body().collect::<Vec<u8>>();

		// Create a str slice from the body.
		let body_str = sp_std::str::from_utf8(&body).map_err(|_| {
			debug::warn!("No UTF8 body");
			http::Error::Unknown
		})?;

		debug::info!("body_str: {}", body_str);
		// Parse the string of data into serde_json::Value.
		let node_info: Vec<NodeInfo> = serde_json::from_str(&body_str).map_err(|_| {
			debug::warn!("No UTF8 body");
			http::Error::Unknown
		})?;

		// debug::info!("Nodes info: {:#?}", node_info);

		for one_node in node_info.iter() {
			debug::info!("Nodes info: {:?}", one_node);
			let resp_bytes = Self::fetch_from_node(one_node).map_err(|e| {
				debug::error!("fetch_from_node error: {:?}", e);
				http::Error::Unknown
			})?;

			let resp_str = sp_std::str::from_utf8(&resp_bytes).map_err(|_| {
				debug::warn!("No UTF8 body");
				http::Error::Unknown
			})?;

			// Print out our fetched JSON string
			debug::info!("Node data: {}", resp_str);

			// Send signed Tx, TODO: send to call SC method.
		}
	
		Ok(())
	}

	// fn parse_topology(topology_str: &str) -> Result<(), http::Error> {
	// 	let val = lite_json::parse_json(topology_str).ok();

	// 	Ok(())

}


// TODO: remove, or write meaningful events.
decl_event!(
	/// Events generated by the module.
	pub enum Event<T> where AccountId = <T as frame_system::Trait>::AccountId {
		NewDdcMetric(AccountId, Vec<u8>),
	}
);


pub trait Trait: frame_system::Trait {
	type ContractId: Get<<<Self::CT as frame_system::Trait>::Lookup as StaticLookup>::Source>;
	type DdcUrl: Get<Vec<u8>>;

	type CT: pallet_contracts::Trait;
	type CST: CreateSignedTransaction<pallet_contracts::Call<Self::CT>>;

	/// The identifier type for an offchain worker.
	type AuthorityId: AppCrypto<<Self::CST as SigningTypes>::Public, <Self::CST as SigningTypes>::Signature>;

	// TODO: remove, or use Event and Call.
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
	/// The overarching dispatch call type.
	type Call: From<Call<Self>>;
}
