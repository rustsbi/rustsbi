# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]


### Added

* Introduces unified secure memory management framework capable of supporting both Penglai/Keystone
* Core traits and structs for implementing unified secure memory management are defined (in lib.rs):
    * The SecMemProtector trait for realizing hardware-based memory isolation.
    * The SecMemAllocator trait for implementing memory management within secure regions.
    * The SecMemManager trait for achieving unified secure memory management.
    * The SecMemRegion struct for managing secure memory.
* A unified SecMemManager implementation based on the allocator/protector examples is provided, which is designed to implement consistent Penglai/Keystone-style secure memory management (in manager.rs).
* Example implementations of first-fit AppAlloc with caller-selected inline record capacity and one-time allocation-based RTAlloc are provided (in allocators.rs).
* The PMP backend reserves one hart-local entry to grant access to a live allocation inside a protected region.

### Modified

* `SecMemManager::extend` is now unsafe: the caller must guarantee the registered range stays valid for writes and unaliased until the manager hands the region back, as the safe sanitization methods `free_em` and `deinit` may zero any part of it.
* `alloc_em` now rejects allocator results outside their originating region.
* The manager requires exclusive ownership of the PMP entries and platform callbacks for synchronized local and all-hart updates. Monitor initialization requires the actual monitor range.
* Local grants are exclusive per hart. An allocation can be freed once its last grant is revoked, including when harts still hold grants on other allocations in the region. Failed hardware updates preserve ownership for retry.
* Freeing a live allocation clears its contents before the allocator reuses the block for bookkeeping. Reclamation clears the entire region before releasing protection.
* Within a region, runtime allocation selects the largest naturally aligned power-of-two block the allocator can supply that satisfies the request. The manager tracks live allocations independently of estimated capacity. A failed first allocation leaves the region available to either allocation type.

### Removed
