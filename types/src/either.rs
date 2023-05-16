/// A type that is either something of type A, or something of type B.
pub(crate) enum Either<L, R> {
    Left(L),
    Right(R),
}
