# SPDX-License-Identifier: MulanPSL-2.0

.PHONY: check test

check:
	for script in scripts/*.sh; do bash -n "$$script" || exit; done
	python3 -B tests/test_setup.py --syntax-only

test:
	python3 -B tests/test_setup.py
