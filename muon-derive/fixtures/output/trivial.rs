#[allow(unused_imports)]
use muon::Observe;
use serde::Serialize;
#[rustfmt::skip]
#[derive(Serialize)]
pub struct Foo<T> {
    a: T,
}
#[rustfmt::skip]
#[automatically_derived]
impl<T> ::muon::Observe for Foo<T> {
    type Observer<'ob, S, N> = ::muon::general::ShallowObserver<'ob, S, N>
    where
        Self: 'ob,
        N: ::muon::helper::Unsigned,
        S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
    type Spec = ::muon::observe::DefaultSpec;
}
#[rustfmt::skip]
#[derive(Serialize)]
pub struct Bar<T> {
    a: Vec<T>,
}
#[rustfmt::skip]
const _: () = {
    pub struct BarSnapshot<T>
    where
        Vec<T>: ::muon::general::Snapshot,
    {
        a: <Vec<T> as ::muon::general::Snapshot>::Snapshot,
    }
    #[automatically_derived]
    impl<T> ::muon::general::Snapshot for Bar<T>
    where
        Vec<T>: ::muon::general::Snapshot,
    {
        type Snapshot = BarSnapshot<T>;
        fn to_snapshot(&self) -> Self::Snapshot {
            BarSnapshot {
                a: ::muon::general::Snapshot::to_snapshot(&self.a),
            }
        }
        #[allow(clippy::match_like_matches_macro)]
        fn eq_snapshot(&self, snapshot: &Self::Snapshot) -> bool {
            ::muon::general::Snapshot::eq_snapshot(&self.a, &snapshot.a)
        }
    }
};
#[rustfmt::skip]
#[automatically_derived]
impl<T> ::muon::Observe for Bar<T>
where
    Self: ::muon::general::Snapshot,
{
    type Observer<'ob, S, N> = ::muon::general::SnapshotObserver<'ob, S, N>
    where
        Self: 'ob,
        N: ::muon::helper::Unsigned,
        S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
    type Spec = ::muon::observe::SnapshotSpec;
}
#[rustfmt::skip]
#[derive(Serialize)]
pub struct NoopStruct {}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::general::Snapshot for NoopStruct {
    type Snapshot = ();
    fn to_snapshot(&self) {}
    fn eq_snapshot(&self, snapshot: &()) -> bool {
        true
    }
}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::Observe for NoopStruct {
    type Observer<'ob, S, N> = ::muon::general::NoopObserver<'ob, S, N>
    where
        Self: 'ob,
        N: ::muon::helper::Unsigned,
        S: ::muon::helper::AsDerefMut<N, Target = Self> + ?Sized + 'ob;
    type Spec = ::muon::observe::SnapshotSpec;
}
