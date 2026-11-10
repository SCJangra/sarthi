use serde::Serialize;
use serde::de::DeserializeOwned;

pub trait RpcMethod {
	const METHOD: &'static str;

	type Params: Serialize;
	type Response: DeserializeOwned;
}
