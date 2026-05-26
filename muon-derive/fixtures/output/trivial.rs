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
        Vec<T>: ::muon::general::SerializeSnapshot,
    {
        a: <Vec<T> as ::muon::general::Snapshot>::Snapshot,
    }
    #[automatically_derived]
    impl<T> ::muon::general::Snapshot for Bar<T>
    where
        Vec<T>: ::muon::general::SerializeSnapshot,
    {
        type Snapshot = BarSnapshot<T>;
        fn to_snapshot(&self) -> Self::Snapshot {
            BarSnapshot {
                a: ::muon::general::Snapshot::to_snapshot(&self.a),
            }
        }
    }
    #[automatically_derived]
    impl<T> ::muon::general::SerializeSnapshot for Bar<T>
    where
        Vec<T>: ::muon::general::SerializeSnapshot,
        Self: ::serde::Serialize,
    {
        fn flush(&self, snapshot: Self::Snapshot) -> ::muon::Mutations {
            let a = ::muon::general::SerializeSnapshot::flush(&self.a, snapshot.a)
                .with_prefix("a");
            if a.is_replace() {
                ::muon::Mutations::replace(self)
            } else {
                let mut mutations = ::muon::Mutations::new();
                mutations.extend(a);
                mutations
            }
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
}
#[rustfmt::skip]
#[automatically_derived]
impl ::muon::general::SerializeSnapshot for NoopStruct {
    fn flush(&self, _snapshot: ()) -> ::muon::Mutations {
        ::muon::Mutations::new()
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
