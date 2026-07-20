# Build both desktop backends by default; packagers may override this, for example
# with `make CARGO_FEATURES=gnome build`.
CARGO_FEATURES ?= gnome,cosmic
ifeq ($(strip $(RUST_TOOLCHAIN)),)
RUST_TOOLCHAIN := 1.93
endif
SUDO_USER_HOME ?= $(shell if test -n "$$SUDO_USER"; then getent passwd "$$SUDO_USER" 2>/dev/null | cut -d: -f6; fi)
RUSTUP_HOME ?= $(if $(strip $(SUDO_USER_HOME)),$(SUDO_USER_HOME)/.rustup,$(HOME)/.rustup)
RUSTUP ?= $(if $(wildcard $(if $(strip $(SUDO_USER_HOME)),$(SUDO_USER_HOME),$(HOME))/.cargo/bin/rustup),$(if $(strip $(SUDO_USER_HOME)),$(SUDO_USER_HOME),$(HOME))/.cargo/bin/rustup,$(shell command -v rustup 2>/dev/null || printf '%s' rustup))
# Run through rustup so a missing requested toolchain cannot silently select the
# system cargo. Packagers may still override CARGO explicitly.
CARGO ?= $(RUSTUP) run $(RUST_TOOLCHAIN) cargo
RUSTC := $(shell RUSTUP_HOME="$(RUSTUP_HOME)" $(RUSTUP) which --toolchain $(RUST_TOOLCHAIN) rustc)
RUSTUP_PATH ?= $(if $(filter /home/%,$(RUSTUP)),$(dir $(RUSTUP)),)

build:
	mkdir -p debian/tmp_files/.cargo
	RUSTC="$(RUSTC)" RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER= PATH="$(RUSTUP_PATH)$$PATH" RUSTUP_HOME="$(RUSTUP_HOME)" RUSTUP_TOOLCHAIN="$(RUST_TOOLCHAIN)" CARGO_HOME=debian/tmp_files/.cargo $(CARGO) build --release --no-default-features --features $(CARGO_FEATURES)
