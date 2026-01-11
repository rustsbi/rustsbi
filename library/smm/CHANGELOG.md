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
* Example implementations of buddy-based AppAlloc and one-time allocation-based RTAlloc are provided (in allocators.rs).
* Example implementation of the PMP-based and mock test protectors are provided (in protectors.rs).
* Basic functional and stress tests were conducted on the implementations of the sample allocator/manager.

### Modified

### Removed
