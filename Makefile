# Makefile for hints Rust project

# Variables
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
CARGO ?= cargo
RUSTFLAGS ?=

# Default target
all: build

# Build the project
build:
	$(CARGO) build --release

# Clean build artifacts
clean:
	$(CARGO) clean
	rm -rf target

# Install binaries to $(BINDIR)
install: build
	mkdir -p $(BINDIR)
	install -m 755 target/release/hints $(BINDIR)/
	install -m 755 target/release/hintsd $(BINDIR)/

# Uninstall binaries
uninstall:
	rm -f $(BINDIR)/hints
	rm -f $(BINDIR)/hintsd

# Run tests
test:
	$(CARGO) test

# Format code
fmt:
	$(CARGO) fmt

# Clippy lint
clippy:
	$(CARGO) clippy -- -D warnings

.PHONY: all build clean install uninstall test fmt clippy