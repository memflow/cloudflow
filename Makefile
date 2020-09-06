.PHONY: all release debug test install

all:
	make test
	make release

release:
	cargo build --release --all-features

debug:
	cargo build --all-features

clean:
	cargo clean

test:
	cargo test --all-features

install:
	./install.sh
