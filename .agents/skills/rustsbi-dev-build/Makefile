# SPDX-License-Identifier: MulanPSL-2.0
.PHONY: check test smoke benchmark
check test:
	$(MAKE) -C tests $@
smoke benchmark:
	bash benchmark/run.sh
