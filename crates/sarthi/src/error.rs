//! What can stop sarthi.

use std::ffi::OsString;
use std::fmt;

use crate::startup::MINIMUM;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Renderer({0})")]
	Renderer(#[from] renderer::RendererError),

	#[error("Nvim({0})")]
	Nvim(#[from] rpc::client::Error),

	#[error("CreateWindow({0})")]
	Window(#[from] winit::error::OsError),

	#[error("ThreadPool({0})")]
	ThreadPool(#[from] rayon::ThreadPoolBuildError),
}

/// Why Neovim didn't start.
#[derive(Debug, PartialEq)]
pub enum StartError {
	NotFound {
		nvim: OsString,
	},
	/// Older than [`MINIMUM`]; its version, if it could tell.
	TooOld {
		nvim: OsString,
		version: Option<String>,
	},
	Failed {
		nvim: OsString,
		error: String,
	},
}

impl StartError {
	/// A heading, and what happened and what to do.
	pub fn text(&self) -> (String, String) {
		let (major, minor) = MINIMUM;
		let hint = "or start Sarthi with --nvim /path/to/nvim.";
		match self {
			Self::NotFound { nvim } => (
				String::from("Sarthi couldn't find Neovim"),
				format!("`{}` wasn't found. Install Neovim {major}.{minor} or newer, {hint}", nvim.display()),
			),
			Self::TooOld { nvim, version } => (
				format!("Sarthi needs Neovim {major}.{minor} or newer"),
				format!(
					"`{}` is {}. Update Neovim, {hint}",
					nvim.display(),
					version.as_deref().unwrap_or("older than 0.9")
				),
			),
			Self::Failed { nvim, error } => {
				(String::from("Sarthi couldn't start Neovim"), format!("`{}`: {error}", nvim.display()))
			},
		}
	}
}

impl fmt::Display for StartError {
	fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		let (heading, body) = self.text();
		write!(formatter, "{heading}: {body}")
	}
}
