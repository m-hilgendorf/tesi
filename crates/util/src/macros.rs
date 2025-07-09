/// Cast an expression to a usize using TryInto.
#[macro_export]
macro_rules! cast_usize {
    ($e:expr) => {{
        let Ok(u) = <_ as TryInto<usize>>::try_into($e) else {
            unreachable!("unsupported target architecture")
        };
        u
    }};
}
pub use cast_usize;
