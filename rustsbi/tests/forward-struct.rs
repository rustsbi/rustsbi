use rustsbi::{Forward, RustSBI};

// The `Forward` structure must build

#[derive(RustSBI)]
struct ForwardAll {
    #[rustsbi(
        console, cppc, hsm, ipi, nacl, pmu, reset, fence, sta, susp, timer, info
    )]
    forward: Forward,
}
