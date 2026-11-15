# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

* Reserved PMP slots are never allocated, and the manage mask covers every slot.
* `extend` fails when no PMP slot is free, and rejects regions that overlap the SM region.
* `reclaim` tries each unused region, and `deinit` only reports regions it unprotected.

### Changed

* **Breaking:** `SecMemProtector` gains `with_masks` and `reserve_locked`, its methods take `PmpSlot`, and `SecMemRegion::new` takes `PmpSlot`. `init` reserves PMP slots the hardware already locked, so they are never allocated.
* `UniSecMemManager` is generic over the protector type. It still defaults to the mock protector; the PMP protector is opt-in until all-hart PMP sync exists.


### Added

* Introduces unified secure memory management framework capable of supporting both Penglai/Keystone
* Core traits and structs for implementing unified secure memory management are defined (in lib.rs):
    * The SecMemProtector trait for realizing hardware-based memory isolation.
    * The SecMemAllocator trait for implementing memory management within secure regions.
    * The SecMemManager trait for achieving unified secure memory management.
    * The SecMemRegion struct for managing secure memory.
* A unified SecMemManager implementation based on the allocator/protector examples is provided, which is designed to implement consistent Penglai/Keystone-style secure memory management (in manager.rs).
* Example implementations of first-fit AppAlloc with caller-selected inline record capacity and one-time allocation-based RTAlloc are provided (in allocators.rs).
* Example implementation of the PMP-based and mock test protectors are provided (in protectors.rs).
* Basic functional and stress tests were conducted on the implementations of the sample allocator/manager.

### Modified

* `SecMemManager::extend` is now unsafe: the caller must guarantee the registered range stays valid for writes and unaliased until the manager hands the region back, as the safe sanitization methods `free_em` and `deinit` may zero any part of it.
* `alloc_em` now rejects allocator results outside their originating region.

### Removed
