# oxGBC — common developer tasks.
# Run `make` (or `make help`) to list targets.
#
# Brand assets (icons, logo, favicons) are generated from a separate private
# repo and committed here. To regenerate them, clone that repo and run
# `OXGBC_ROOT=$(pwd) make icons` from its oxgbc/ directory.

PORT ?= 8080
WEB  := crates/web

.DEFAULT_GOAL := help
.PHONY: help serve web bench-ab

help: ## List available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

serve: ## Serve the web frontend at http://localhost:$(PORT)
	python3 -m http.server -d $(WEB) $(PORT)

web: ## Build the WASM module + JS bindings into crates/web/pkg
	./$(WEB)/build.sh

bench-ab: ## Interleaved A/B bench matrix: make bench-ab A=<cli> [B=<cli>] [PAIRS=5]; games via OXGBC_BENCH_GB_ROM/OXGBC_BENCH_GBC_ROM
	@A="$(A)" B="$(B)" PAIRS="$(or $(PAIRS),5)" ./scripts/bench-ab.sh
