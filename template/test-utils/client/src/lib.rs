pub use substrate_test_client::*;
pub use frontier_template_runtime as runtime;
use sp_runtime::traits::HashFor;

sc_executor::native_executor_instance! {
	pub LocalExecutor,
	runtime::api::dispatch,
	runtime::native_version,
}

pub type Backend = substrate_test_client::Backend<runtime::Block>;

pub type Executor = client::LocalCallExecutor<
	Backend,
	NativeExecutor<LocalExecutor>,
>;

pub type LightBackend = substrate_test_client::LightBackend<runtime::Block>;

pub type LightExecutor = sc_light::GenesisCallExecutor<
	LightBackend,
	client::LocalCallExecutor<
		sc_light::Backend<
			sc_client_db::light::LightStorage<runtime::Block>,
			HashFor<runtime::Block>
		>,
		NativeExecutor<LocalExecutor>
	>
>;
