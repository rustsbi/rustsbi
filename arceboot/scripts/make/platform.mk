# Architecture and platform resolving

ifeq ($(PLATFORM),)
  # `PLATFORM` is not specified, use the default platform for each architecture
  ifeq ($(ARCH), x86_64)
    PLAT_NAME := x86_64-qemu-q35
  else ifeq ($(ARCH), aarch64)
    PLAT_NAME := aarch64-qemu-virt
  else ifeq ($(ARCH), riscv64)
    PLAT_NAME := riscv64-qemu-virt
  else ifeq ($(ARCH), loongarch64)
    PLAT_NAME := loongarch64-qemu-virt
  else
    $(error "ARCH" must be one of "x86_64", "riscv64", "aarch64" or "loongarch64")
  endif
  PLAT_CONFIG := configs/platforms/$(PLAT_NAME).toml
else
  # `PLATFORM` is specified, override the `ARCH` variables
  builtin_platforms := $(patsubst configs/platforms/%.toml,%,$(wildcard configs/platforms/*))
  ifneq ($(filter $(PLATFORM),$(builtin_platforms)),)
    # builtin platform
    _arch := $(word 1,$(subst -, ,$(PLATFORM)))
    PLAT_NAME := $(PLATFORM)
    PLAT_CONFIG := configs/platforms/$(PLAT_NAME).toml
  else ifneq ($(wildcard $(PLATFORM)),)
    # custom platform, read the "arch" and "plat-name" fields from the toml file
    _arch :=  $(patsubst "%",%,$(shell axconfig-gen $(PLATFORM) -r arch))
    PLAT_NAME := $(patsubst "%",%,$(shell axconfig-gen $(PLATFORM) -r platform))
    PLAT_CONFIG := $(PLATFORM)
  else
    $(error "PLATFORM" must be one of "$(builtin_platforms)" or a valid path to a toml file)
  endif
  ifeq ($(origin ARCH),command line)
    ifneq ($(ARCH),$(_arch))
      $(error "ARCH=$(ARCH)" is not compatible with "PLATFORM=$(PLATFORM)")
    endif
  endif
  ARCH := $(_arch)
endif
