//! What can stop sarthi

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
