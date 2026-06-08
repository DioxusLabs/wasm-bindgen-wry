use super::UpcastFrom;

macro_rules! impl_fn_upcasts {
    () => {
        impl_fn_upcasts!(@arities
            [0 []]
            [1 [A1 B1] O1]
            [2 [A1 B1 A2 B2] O2]
            [3 [A1 B1 A2 B2 A3 B3] O3]
            [4 [A1 B1 A2 B2 A3 B3 A4 B4] O4]
            [5 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5] O5]
            [6 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5 A6 B6] O6]
            [7 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5 A6 B6 A7 B7] O7]
            [8 [A1 B1 A2 B2 A3 B3 A4 B4 A5 B5 A6 B6 A7 B7 A8 B8] O8]
        );
    };

    (@arities) => {};

    (@arities [$n:tt $args:tt $($opt:ident)?] $([$rest_n:tt $rest_args:tt $($rest_opt:ident)?])*) => {
        impl_fn_upcasts!(@same $args);
        impl_fn_upcasts!(@cross_all $args [] $([$rest_n $rest_args $($rest_opt)?])*);
        impl_fn_upcasts!(@arities $([$rest_n $rest_args $($rest_opt)?])*);
    };

    (@same []) => {
        impl<R1, R2> UpcastFrom<fn() -> R1> for fn() -> R2
        where
            R2: UpcastFrom<R1>,
        {
        }

        impl<'a, R1, R2> UpcastFrom<dyn Fn() -> R1 + 'a> for dyn Fn() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
        {
        }

        impl<'a, R1, R2> UpcastFrom<dyn FnMut() -> R1 + 'a> for dyn FnMut() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
        {
        }
    };

    (@same [$($A1:ident $A2:ident)+]) => {
        impl<R1, R2, $($A1, $A2),+> UpcastFrom<fn($($A1),+) -> R1> for fn($($A2),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2),+> UpcastFrom<dyn Fn($($A1),+) -> R1 + 'a> for dyn Fn($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2),+> UpcastFrom<dyn FnMut($($A1),+) -> R1 + 'a> for dyn FnMut($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
        {
        }
    };

    (@cross_all $args:tt $opts:tt) => {};

    (@cross_all $args:tt [$($opts:ident)*] [$next_n:tt $next_args:tt $next_opt:ident] $([$rest_n:tt $rest_args:tt $($rest_opt:ident)?])*) => {
        impl_fn_upcasts!(@extend $args [$($opts)* $next_opt]);
        impl_fn_upcasts!(@shrink $args [$($opts)* $next_opt]);
        impl_fn_upcasts!(@cross_all $args [$($opts)* $next_opt] $([$rest_n $rest_args $($rest_opt)?])*);
    };

    (@extend [] [$($O:ident)+]) => {
        impl<R1, R2, $($O),+> UpcastFrom<fn() -> R1> for fn($($O),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn Fn() -> R1 + 'a> for dyn Fn($($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn FnMut() -> R1 + 'a> for dyn FnMut($($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };

    (@extend [$($A1:ident $A2:ident)+] [$($O:ident)+]) => {
        impl<R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<fn($($A1),+) -> R1> for fn($($A2,)+ $($O),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn Fn($($A1),+) -> R1 + 'a> for dyn Fn($($A2,)+ $($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn FnMut($($A1),+) -> R1 + 'a> for dyn FnMut($($A2,)+ $($O),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };

    (@shrink [] [$($O:ident)+]) => {
        impl<R1, R2, $($O),+> UpcastFrom<fn($($O),+) -> R1> for fn() -> R2
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn Fn($($O),+) -> R1 + 'a> for dyn Fn() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($O),+> UpcastFrom<dyn FnMut($($O),+) -> R1 + 'a> for dyn FnMut() -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };

    (@shrink [$($A1:ident $A2:ident)+] [$($O:ident)+]) => {
        impl<R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<fn($($A1,)+ $($O),+) -> R1> for fn($($A2),+) -> R2
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn Fn($($A1,)+ $($O),+) -> R1 + 'a> for dyn Fn($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }

        impl<'a, R1, R2, $($A1, $A2,)+ $($O),+> UpcastFrom<dyn FnMut($($A1,)+ $($O),+) -> R1 + 'a> for dyn FnMut($($A2),+) -> R2 + 'a
        where
            R2: UpcastFrom<R1>,
            $($A1: UpcastFrom<$A2>,)+
            $($O: UpcastFrom<crate::sys::Undefined>,)+
        {
        }
    };
}

impl_fn_upcasts!();

// A `ScopedClosure` upcasts wherever its underlying closure type does. Return
// covariance and argument contravariance both reduce to an upcast of the inner
// closure signature. All `ScopedClosure`s erase to the same repr, so the pointer
// cast in `Upcast::upcast` is sound.
impl<'a, T: ?Sized, U: ?Sized> UpcastFrom<crate::ScopedClosure<'a, U>>
    for crate::ScopedClosure<'a, T>
where
    T: UpcastFrom<U>,
{
}
