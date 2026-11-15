# Design

### Give each unit a single responsibility (`single-responsibility`) {#single-responsibility}

Each module, type, or function
should have one, and only one, reason to change.
If you cannot describe what a unit does
without the words "and," "or," or "but,"
it has too many responsibilities.

This applies at every scale:

- **Functions** should do one thing, do it well, and do it only.
  If you can extract another function from it
  with a name that is not merely a restatement of its implementation,
  the original function is doing more than one thing.
  Do not mix levels of abstraction:
  a syscall handler should read like a specification,
  while byte-level manipulation belongs in a helper.
- **Files** should hold one concept.
  When a file grows long or contains multiple distinct concepts,
  split it — each major data structure, subsystem entry point,
  or significant abstraction deserves its own file.

```rust
// Good — the handler reads like a specification;
// every step is one call at the syscall level of abstraction
// (from Asterinas kernel/core/src/syscall/connect.rs)
pub(super) fn sys_connect(
    sockfd: RawFileDesc,
    sockaddr_ptr: Vaddr,
    addr_len: u32,
    ctx: &Context,
) -> Result<SyscallReturn> {
    let socket_addr = read_socket_addr_from_user(sockaddr_ptr, addr_len as _)?;

    let mut file_table = ctx.thread_local.borrow_file_table_mut();
    let file = get_file_fast!(&mut file_table, sockfd.try_into()?);
    let socket = file.as_socket_or_err()?;

    socket.connect(socket_addr)?;
    Ok(SyscallReturn::Return(0))
}

// Bad — the same handler inlines byte-level address parsing,
// so the syscall flow and the C-layout decoding must be reviewed together
pub(super) fn sys_connect(
    sockfd: RawFileDesc,
    sockaddr_ptr: Vaddr,
    addr_len: u32,
    ctx: &Context,
) -> Result<SyscallReturn> {
    let mut storage = Storage::new_zeroed();
    current_userspace!().read_bytes(
        sockaddr_ptr,
        &mut storage.as_mut_bytes()[..addr_len],
    )?;
    let socket_addr = match CSocketAddrFamily::try_from(storage.sa_family as i32) {
        Ok(CSocketAddrFamily::AF_INET) => {
            let (addr, port) = CSocketAddrInet::from_first_bytes(storage.as_bytes()).into();
            SocketAddr::IPv4(addr, port)
        }
        Ok(CSocketAddrFamily::AF_INET6) => {
            let (addr, port) = CSocketAddrInet6::from_first_bytes(storage.as_bytes()).into();
            SocketAddr::IPv6(addr, port)
        }
        // ... plus AF_UNIX, AF_NETLINK, AF_VSOCK, and the error paths
        _ => return_errno_with_message!(Errno::EINVAL, "unsupported address family"),
    };

    let mut file_table = ctx.thread_local.borrow_file_table_mut();
    let file = get_file_fast!(&mut file_table, sockfd.try_into()?);
    let socket = file.as_socket_or_err()?;

    socket.connect(socket_addr)?;
    Ok(SyscallReturn::Return(0))
}
```

The good version is
[Asterinas's `sys_connect`](https://github.com/asterinas/asterinas/blob/main/kernel/core/src/syscall/connect.rs);
the byte-level parsing inlined in the bad version
is exactly what Asterinas keeps in the helper
`read_socket_addr_from_user`,
which every socket syscall reuses,
so each handler stays at one level of abstraction.

### Hide details behind interfaces (`information-hiding`) {#information-hiding}

Hide details behind well-defined interfaces.
A module's public surface should contain
only what its consumers need.
Internal data structures, helper types,
and bookkeeping fields should remain private,
and implementation details should not leak
through public APIs (including their documentation).

### Be open for extension, closed for modification (`open-closed`) {#open-closed}

Stable modules and APIs should be
open to extension
but closed to breaking modification.
Prefer adding new behavior
through existing interfaces
(traits, enums, and pluggable components)
instead of repeatedly editing established call paths.
Do not introduce extension points preemptively;
add them when there is a concrete extension need.

### Aim for loose coupling and strong cohesion (`coupling-cohesion`) {#coupling-cohesion}

Connections between modules should be
small, visible, and flexible.
Within a module, every part should contribute
to a single, well-defined purpose.

### Be consistent (`consistency`) {#consistency}

Do similar things the same way throughout the codebase.
Consistency reduces surprise and cognitive load
even when neither approach is objectively superior.
When a convention already exists, follow it;
do not introduce a competing convention
without compelling justification.
