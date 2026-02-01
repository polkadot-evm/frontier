struct PrecompileAt<T, U, V = ()>(PhantomData<(T, U, V)>);
struct AddressU64<const N: u64>;
struct FooPrecompile<R>(PhantomData<R>);
struct BarPrecompile<R, S>(PhantomData<(R, S)>);
struct MockCheck;
type Precompiles = (
    PrecompileAt<AddressU64<1>, FooPrecompile<R>>,
    PrecompileAt<AddressU64<2>, BarPrecompile<R, S>, (MockCheck, MockCheck)>,
);
#[repr(u64)]
pub enum PrecompileName {
    FooPrecompile = 1u64,
    BarPrecompile = 2u64,
}
impl ::num_enum::TryFromPrimitive for PrecompileName {
    type Primitive = u64;
    type Error = ::num_enum::TryFromPrimitiveError<Self>;
    const NAME: &'static str = "PrecompileName";
    fn try_from_primitive(
        number: Self::Primitive,
    ) -> ::core::result::Result<Self, ::num_enum::TryFromPrimitiveError<Self>> {
        #![allow(non_upper_case_globals)]
        const FooPrecompile__num_enum_0__: u64 = 1u64;
        const BarPrecompile__num_enum_0__: u64 = 2u64;
        #[deny(unreachable_patterns)]
        match number {
            FooPrecompile__num_enum_0__ => {
                ::core::result::Result::Ok(Self::FooPrecompile)
            }
            BarPrecompile__num_enum_0__ => {
                ::core::result::Result::Ok(Self::BarPrecompile)
            }
            #[allow(unreachable_patterns)]
            _ => {
                ::core::result::Result::Err(
                    ::num_enum::TryFromPrimitiveError::<Self>::new(number),
                )
            }
        }
    }
}
impl ::core::convert::TryFrom<u64> for PrecompileName {
    type Error = ::num_enum::TryFromPrimitiveError<Self>;
    #[inline]
    fn try_from(
        number: u64,
    ) -> ::core::result::Result<Self, ::num_enum::TryFromPrimitiveError<Self>> {
        ::num_enum::TryFromPrimitive::try_from_primitive(number)
    }
}
#[doc(hidden)]
impl ::num_enum::CannotDeriveBothFromPrimitiveAndTryFromPrimitive for PrecompileName {}
impl From<PrecompileName> for u64 {
    #[inline]
    fn from(enum_value: PrecompileName) -> Self {
        enum_value as Self
    }
}
#[automatically_derived]
impl ::core::fmt::Debug for PrecompileName {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        ::core::fmt::Formatter::write_str(
            f,
            match self {
                PrecompileName::FooPrecompile => "FooPrecompile",
                PrecompileName::BarPrecompile => "BarPrecompile",
            },
        )
    }
}
impl PrecompileName {
    pub fn from_address(address: sp_core::H160) -> Option<Self> {
        let _u64 = address.to_low_u64_be();
        if address == sp_core::H160::from_low_u64_be(_u64) {
            use num_enum::TryFromPrimitive;
            Self::try_from_primitive(_u64).ok()
        } else {
            None
        }
    }
}
