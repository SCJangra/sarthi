use crate::RpcMethod;

pub struct Eval;

impl RpcMethod for Eval {
	const METHOD: &'static str = "nvim_eval";

	type Params = [String; 1];
	type Response = rmpv::Value;
}
