SHELL := /bin/bash

PRODUCT_NAME := Agent Sessions
VERSION := $(shell node -p "require('./package.json').version")
ARCH := $(shell uname -m | sed -e 's/^arm64$$/aarch64/' -e 's/^x86_64$$/x64/')

BUNDLE_DIR := src-tauri/target/release/bundle
APP_BUNDLE := $(BUNDLE_DIR)/macos/$(PRODUCT_NAME).app
DMG_DIR := $(BUNDLE_DIR)/dmg
DMG_NAME := $(PRODUCT_NAME)_$(VERSION)_$(ARCH).dmg
DMG_PATH := $(DMG_DIR)/$(DMG_NAME)
INSTALL_APP := /Applications/$(PRODUCT_NAME) $(VERSION).app

.PHONY: help install dev build test test-js test-rust test-all tauri-build dmg dmg-fallback install-dmg open-installed clean

help:
	@echo "Targets:"
	@echo "  make install        Install npm dependencies"
	@echo "  make dev            Start the Vite dev server"
	@echo "  make build          Build the frontend"
	@echo "  make test           Run JS and Rust tests"
	@echo "  make tauri-build    Build the Tauri app and bundles"
	@echo "  make dmg            Build a macOS DMG, with fallback for Finder scripting failures"
	@echo "  make install-dmg    Mount the DMG and install side-by-side to /Applications"
	@echo "  make open-installed Launch the side-by-side installed app"
	@echo "  make clean          Remove frontend and Tauri build outputs"

install:
	npm install

dev:
	npm run dev

build:
	npm run build

test: test-all

test-js:
	npm test -- --run

test-rust:
	cd src-tauri && cargo test

test-all: test-js test-rust

tauri-build:
	npm run tauri -- build

dmg:
	rm -f "$(DMG_PATH)"
	if ! npm run tauri -- build; then \
		test -x "$(DMG_DIR)/bundle_dmg.sh"; \
		test -d "$(APP_BUNDLE)"; \
		cd "$(DMG_DIR)" && bash ./bundle_dmg.sh --skip-jenkins "$(DMG_NAME)" "../macos/$(PRODUCT_NAME).app"; \
	fi
	test -f "$(DMG_PATH)"
	@echo "DMG ready: $(DMG_PATH)"

dmg-fallback:
	test -x "$(DMG_DIR)/bundle_dmg.sh"
	test -d "$(APP_BUNDLE)"
	cd "$(DMG_DIR)" && bash ./bundle_dmg.sh --skip-jenkins "$(DMG_NAME)" "../macos/$(PRODUCT_NAME).app"

install-dmg: dmg
	hdiutil attach "$(DMG_PATH)" -nobrowse
	ditto "/Volumes/$(PRODUCT_NAME)_$(VERSION)_$(ARCH)/$(PRODUCT_NAME).app" "$(INSTALL_APP)"
	hdiutil detach "/Volumes/$(PRODUCT_NAME)_$(VERSION)_$(ARCH)"
	@echo "Installed: $(INSTALL_APP)"

open-installed:
	open -n "$(INSTALL_APP)"

clean:
	rm -rf dist src-tauri/target
