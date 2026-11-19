# Startup and Handoff

### Publish shared state before releasing harts (`publish-before-release`) {#publish-before-release}

Any state that every hart relies on — the hart context, the trap vectors,
the published device tree or platform info — must be fully written and
made visible *before* secondary harts are released or a trap vector is
installed. Order the startup sequence so the publication of shared state
precedes both trap initialization and the release of secondary harts;
a hart that starts early reads half-built state and the failure is
timing-dependent.

```rust
// Good — order is load-bearing: publish, then trap init, then release
publish_hart_context(&contexts);      // 1. shared state visible to all harts
install_trap_vectors();               // 2. traps may now arrive
release_secondary_harts(&parked);     // 3. other harts observe 1 and 2
```

```rust
// Bad — harts race the publication they depend on
release_secondary_harts(&parked);     // harts read contexts mid-write
publish_hart_context(&contexts);
install_trap_vectors();
```
