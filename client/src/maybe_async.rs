use std::{convert::Infallible, future::{self, Future}, ops::FromResidual};

#[macro_export]
macro_rules! MaybeAsync {
	($t: ty) => {
		MaybeAsync<$t, impl Future<Output = $t>>
	};
}

pub enum MaybeAsync<T, F = future::Ready<T>>
where
	F: Future<Output = T>,
{
	Sync(T),
	Async(F),
}

use either::Either;
use MaybeAsync::*;

impl<T, F> MaybeAsync<T, F>
where
	F: Future<Output = T>,
{
	pub fn map<C, R>(self, callback: C) -> MaybeAsync!(R)
	where
		C: FnOnce(T) -> R,
	{
		match self {
			Sync(value) => Sync(callback(value)),
			Async(future) => Async(async {
				let value = future.await;
				callback(value)
			}),
		}
	}
	
	pub fn chain<C, R, RF>(self, callback: C) -> MaybeAsync!(R)
	where
		C: FnOnce(T) -> MaybeAsync<R, RF>,
		RF: Future<Output = R>,
	{
		self.map(callback).flatten()
	}
	
	pub async fn value(self) -> T {
		match self {
			Self::Sync(value) => value,
			Self::Async(future) => future.await,
		}
	}
}

impl<T, F, MF> MaybeAsync<MaybeAsync<T, F>, MF>
where
	F: Future<Output = T>,
	MF: Future<Output = MaybeAsync<T, F>>,
{
	pub fn flatten(self) -> MaybeAsync!(T) {
		match self {
			Sync(Sync(value)) => Sync(value),
			Sync(Async(future)) => Async(Either::Left(future)),
			Async(future) => Async(Either::Right(async {
				match future.await {
					Sync(value) => value,
					Async(future) => future.await,
				}
			}))
		}
	}
}

impl<T, R, E, F> FromResidual<Result<Infallible, R>> for MaybeAsync<Result<T, E>, F>
where
	E: From<R>,
	F: Future<Output = Result<T, E>>,
{
	fn from_residual(residual: Result<Infallible, R>) -> Self {
		match residual {
			Err(err) => Self::Sync(Err(err.into())),
		}
	}
}
