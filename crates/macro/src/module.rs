#[macro_export]
macro_rules! mod_flat {
    ($($name:ident),+ $(,)?) => {
        $(
            mod $name;
            pub use $name::*;
        )+
    };
}

#[macro_export]
macro_rules! mod_prv {
    ($($name:ident),+ $(,)?) => {
        $(
            mod $name;
        )+
    };
}

#[macro_export]
macro_rules! mod_pub {
    (crate, $($name:ident),+ $(,)?) => {
        $(
            pub(crate) mod $name;
        )+
    };
    (super, $($name:ident),+ $(,)?) => {
        $(
            pub(super) mod $name;
        )+
    };
    ($vis:vis, $($name:ident),+ $(,)?) => {
        $(
            $vis mod $name;
        )+
    };
    ($($name:ident),+ $(,)?) => {
        $(
            pub mod $name;
        )+
    };
}

#[macro_export]
macro_rules! mod_pub_flat {
    ($($name:ident),+ $(,)?) => {
        $(
            pub mod $name;
            pub use $name::*;
        )+
    };
}
