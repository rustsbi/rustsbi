# CI cache policy

Use the Actions cache only for expensive build products that are shared by many
runs and pull requests.

- Download immutable upstream releases, images, source archives, toolchains,
  and SDKs from their official source on every run. Do not spend the 10 GiB
  Actions cache budget on assets that the network can fetch reliably.
- Write caches only on `refs/heads/main`. Pull requests restore those entries
  read-only through `ci-cache`; `ci-cache-save` refuses to write on other refs.
- Key caches by the inputs that affect the product, not by the run or commit.
  Avoid `github.sha` and matrix dimensions unless the cached bytes differ.
- When jobs in one pull-request workflow must exchange generated files, use a
  short-lived artifact as an optimization. Do not create a PR-scoped cache for
  that handoff, and let consumers retain their direct download fallback.
- Cache only the final reusable product. For example, cache a completed binary
  rather than its source and object tree, and omit downloaded inputs that can
  be fetched again.
- Build pinned U-Boot and EDK II through the shared CI scripts and cache their
  final products. Avoid distro packages whose repositories can disappear even
  when their version remains pinned.
