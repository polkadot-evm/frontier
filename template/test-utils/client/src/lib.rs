use std::sync::Arc;

pub use substrate_test_client::*;
pub use frontier_template_runtime as runtime;
use sp_runtime::traits::HashFor;

sc_executor::native_executor_instance! {
	pub LocalExecutor,
	runtime::api::dispatch,
	runtime::native_version,
}

pub type Backend = substrate_test_client::Backend<runtime::opaque::Block>;

pub type Executor = client::LocalCallExecutor<
	Backend,
	NativeExecutor<LocalExecutor>,
>;

pub type LightBackend = substrate_test_client::LightBackend<runtime::opaque::Block>;

pub type LightExecutor = sc_light::GenesisCallExecutor<
	LightBackend,
	client::LocalCallExecutor<
		sc_light::Backend<
			sc_client_db::light::LightStorage<runtime::opaque::Block>,
			HashFor<runtime::opaque::Block>
		>,
		NativeExecutor<LocalExecutor>
	>
>;

/// Parameters of test-client builder with test-runtime.
#[derive(Default)]
pub struct GenesisParameters;

impl substrate_test_client::GenesisInit for GenesisParameters {
	fn genesis_storage(&self) -> Storage {
		Storage::default()
	}
}

/// A `TestClient` with `test-runtime` builder.
pub type TestClientBuilder<E, B> = substrate_test_client::TestClientBuilder<
	runtime::opaque::Block,
	E,
	B,
	GenesisParameters,
>;

/// Test client type with `LocalExecutor` and generic Backend.
pub type Client<B> = client::Client<
	B,
	client::LocalCallExecutor<B, sc_executor::NativeExecutor<LocalExecutor>>,
	runtime::opaque::Block,
	runtime::RuntimeApi,
>;

/// A test client with default backend.
pub type TestClient = Client<Backend>;

/// A `TestClientBuilder` with default backend and executor.
pub trait DefaultTestClientBuilderExt: Sized {
	/// Create new `TestClientBuilder`
	fn new() -> Self;
}

impl DefaultTestClientBuilderExt for TestClientBuilder<Executor, Backend> {
	fn new() -> Self {
		Self::with_default_backend()
	}
}

/// A `test-runtime` extensions to `TestClientBuilder`.
pub trait TestClientBuilderExt<B>: Sized {
	/// Build the test client.
	fn build(self) -> Client<B> {
		self.build_with_longest_chain().0
	}

	/// Build the test client and longest chain selector.
	fn build_with_longest_chain(self) -> (Client<B>, sc_consensus::LongestChain<B, runtime::opaque::Block>);

	/// Build the test client and the backend.
	fn build_with_backend(self) -> (Client<B>, Arc<B>);
}

impl<B> TestClientBuilderExt<B> for TestClientBuilder<
	client::LocalCallExecutor<B, sc_executor::NativeExecutor<LocalExecutor>>,
	B
> where
	B: sc_client_api::backend::Backend<runtime::opaque::Block> + 'static,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	<B as sc_client_api::backend::Backend<runtime::opaque::Block>>::State:
		sp_api::StateBackend<HashFor<runtime::opaque::Block>>,
{
	fn build_with_longest_chain(self) -> (Client<B>, sc_consensus::LongestChain<B, runtime::opaque::Block>) {
		self.build_with_native_executor(None)
	}

	fn build_with_backend(self) -> (Client<B>, Arc<B>) {
		let backend = self.backend();
		(self.build_with_native_executor(None).0, backend)
	}
}

/// Creates new client instance used for tests.
pub fn new() -> Client<Backend> {
	TestClientBuilder::new().build()
}
