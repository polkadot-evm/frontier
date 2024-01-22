use super::*;

#[allow(unused)]
use crate::Pallet as EVMPrecompile;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;

benchmarks! {
	add_precompile {
		let address = H160::from_low_u64_be(1);
		let label = PrecompileLabel::new(
			b"SomePrecompileLabel"
				.to_vec()
				.try_into()
				.expect("less than 32 chars; qed"),
		);

	}: _(RawOrigin::Root, address, label.clone())
	verify {
		let read_precompile = EVMPrecompile::<T>::precompiles(address);
		assert_eq!(read_precompile, label);
	}

	remove_precompile {
		let address = H160::from_low_u64_be(1);
		let label = PrecompileLabel::new(
			b"SomePrecompileLabel"
				.to_vec()
				.try_into()
				.expect("less than 32 chars; qed"),
		);
		EVMPrecompile::<T>::add_precompile(RawOrigin::Root.into(), address, label).unwrap();
	}: _(RawOrigin::Root, address)
	verify {
		let read_precompile = EVMPrecompile::<T>::precompiles(address);
		assert_eq!(read_precompile, PrecompileLabel::default());
	}
}

impl_benchmark_test_suite!(
	EVMPrecompile,
	crate::tests::new_test_ext(),
	crate::mock::Test
);
