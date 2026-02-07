# build.mk

TARGET := riscv64gc-unknown-none-elf
SCRIPT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
ROOT_DIR := $(abspath $(SCRIPT_DIR)/../..)

ELF := $(ROOT_DIR)/target/$(TARGET)/release/arceboot
BIN := $(ROOT_DIR)/target/$(TARGET)/release/arceboot.bin
RUSTSBI_DIR := $(ROOT_DIR)/rustsbi

# 用逗号或者空格分隔 feature 名称
EXACT_FEATURES ?=
EXACT_FEATURES_CLEANED := $(subst $(space),$(comma),$(strip $(EXACT_FEATURES)))

ifeq ($(strip $(EXACT_FEATURES)),)
  FEATURE_ARGS :=
else
  FEATURE_ARGS := --features '$(EXACT_FEATURES_CLEANED)'
endif

# 彩色打印
define print_info
	@printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build] "
	@printf "\033[1;32m%s\033[0m" "[INFO] "
	@printf "\033[36m%s\033[0m\n" "$(1)"
endef

.PHONY: all build-arceboot extract-bin clone-rustsbi build-rustsbi

all: build-arceboot extract-bin clone-rustsbi build-rustsbi
	$(call print_info,所有任务执行完成 项目构建成功 目标 elf 文件位于: $(SBI))

build-arceboot:
	$(call print_info,开始编译 ArceBoot...)
	cargo rustc --release --target $(TARGET) $(FEATURE_ARGS) -- \
		-C opt-level=z \
		-C panic=abort \
		-C relocation-model=static \
		-C target-cpu=generic \
		-C link-args="-Tlink.ld"
	$(call print_info,编译 ArceBoot 成功)

extract-bin: $(ELF)
	$(call print_info,开始提取 ArceBoot Binary...)
	rust-objcopy --binary-architecture=riscv64 $< --strip-all -O binary $(BIN)
	$(call print_info,提取 ArceBoot Binary 成功)

clone-rustsbi:
	@if [ ! -d "$(RUSTSBI_DIR)" ]; then \
		printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build] "; \
		printf "\033[1;32m%s\033[0m" "[INFO] "; \
		printf "\033[36m%s\033[0m\n" "仓库 rustsbi 不存在，正在克隆仓库..."; \
		git clone https://github.com/rustsbi/rustsbi.git $(RUSTSBI_DIR); \
		printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build] "; \
		printf "\033[1;32m%s\033[0m" "[INFO] "; \
		printf "\033[36m%s\033[0m\n" "仓库 rustsbi 克隆成功"; \
	else \
		printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build] "; \
		printf "\033[1;32m%s\033[0m" "[INFO] "; \
		printf "\033[36m%s\033[0m\n" "仓库 rustsbi 已存在"; \
	fi

build-rustsbi:
	$(call print_info,开始以 payload 形式编译 rustsbi...)
	cd rustsbi && cargo prototyper --payload $(BIN)
	$(call print_info,rustsbi 编译成功)