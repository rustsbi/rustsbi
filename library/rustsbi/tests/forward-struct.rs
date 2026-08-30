use rustsbi::{Forward, RustSBI};

// The `Forward` structure must build

#[allow(unused)] // FIXME: hot fix, use it on unit test in the future.
#[derive(RustSBI)]
struct ForwardAll {
    #[rustsbi(
        console, cppc, hsm, ipi, nacl, pmu, reset, fence, sse, sta, susp, timer, info
    )]
    forward: Forward,
}
