use serde::Deserialize;

use crate::RpcMethod;

/// Neovim's [`nvim_get_chan_info`](https://neovim.io/doc/user/api/#nvim_get_chan_info()) function: what Neovim knows
/// about a channel, 0 for the one calling
pub struct GetChanInfo;

impl RpcMethod for GetChanInfo {
	const METHOD: &'static str = "nvim_get_chan_info";

	type Params = [u64; 1];
	type Response = ChannelInfo;
}

/// The parts of a channel's info sarthi uses; Neovim sends more
#[derive(Debug, Deserialize)]
pub struct ChannelInfo {
	pub id: u64,
}
