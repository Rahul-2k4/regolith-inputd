# Build both desktop backends by default; packagers may override this, for example
# with `make CARGO_FEATURES=gnome build`.
CARGO_FEATURES ?= gnome,cosmic

build:
	mkdir -p debian/tmp_files/.cargo
	CARGO_HOME=debian/tmp_files/.cargo cargo build --release --no-default-features --features $(CARGO_FEATURES)
