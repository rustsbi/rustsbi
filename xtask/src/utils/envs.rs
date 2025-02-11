#[allow(unused)]
macro_rules! export_env {
    ($env:literal ?= $val:expr) => {
        if std::env::vars_os().all(|(k, _)| k != $env) {
            std::env::set_var($env, $val);
        }
    };
    ($env0:literal ?= $val0:expr, $($env:literal ?= $val:expr,)+) => {
        export_env!($env0 ?= $val0);
        $(
            export_env!($env ?= $val);
        )+
    };
}
