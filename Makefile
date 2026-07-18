# Build both desktop backends by default; packagers may override this, for example
# with `make CARGO_FEATURES=gnome build`.
CARGO_FEATURES ?= gnome,cosmic
ifeq ($(strip $(RUST_TOOLCHAIN)),)
RUST_TOOLCHAIN := 1.93
endif
RUSTUP ?= rustup
CARGO ?= $(shell toolchain=$$(if command -v "$(RUSTUP)" >/dev/null 2>&1; then "$(RUSTUP)" toolchain list 2>/dev/null | awk -v prefix="$(RUST_TOOLCHAIN)" '$$1 ~ ("^" prefix "([.-]|$$)") { print $$1; exit }'; fi); if test -n "$$toolchain"; then printf '%s' "$(RUSTUP) run $$toolchain cargo"; else printf '%s' cargo; fi)

build:
	mkdir -p debian/tmp_files/.cargo
	CARGO_HOME=debian/tmp_files/.cargo $(CARGO) build --release --no-default-features --features $(CARGO_FEATURES)
