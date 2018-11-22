#![no_std]

use core::ops::Deref;

/// A type for storing owned or borrowed data that has a common type.
/// Useful for returning either a borrow or owned data from a function.
pub enum CowLike<'a, A: 'a + ?Sized, B> {
	Borrowed(&'a A),
	Owned(B),
}

impl<'a, A: ?Sized, B> Deref for CowLike<'a, A, B> where B: AsRef<A> {
	type Target = A;
	fn deref(&self) -> &A {
		match self {
			CowLike::Borrowed(b) => b,
			CowLike::Owned(o) => o.as_ref(),
		}
	}
}
