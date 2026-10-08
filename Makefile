PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
DATADIR := $(PREFIX)/share
ICONDIR := $(DATADIR)/icons/hicolor
AUTOSTARTDIR := $(or $(XDG_CONFIG_HOME),$(HOME)/.config)/autostart
# GNOME Shell reads search providers from system data dirs only.
SEARCH_PROVIDER_DIR := /usr/local/share/gnome-shell/search-providers
APP_ID := io.github.cszach.Muse

# Data files with the install path filled in.
GENDIR := target/data
GENERATED := $(GENDIR)/$(APP_ID).desktop $(GENDIR)/$(APP_ID).service \
	$(GENDIR)/$(APP_ID).autostart.desktop

# Fallback quick-ask key, for desktops without the GlobalShortcuts portal.
SHORTCUT ?= <Control><Alt>m

# Sizes of the official icon in both the muse.ai manifest and hicolor.
OFFICIAL_SIZES := 512 256 192 96 72 64 48 32

.PHONY: all build run check fmt clippy test validate install uninstall \
	enable-autostart disable-autostart install-shortcut uninstall-shortcut \
	install-search-provider uninstall-search-provider \
	fetch-icon FORCE

all: build

build:
	cargo build --release

run:
	cargo run -- --debug

check: fmt clippy test validate

fmt:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test --all-targets

validate: $(GENERATED)
	desktop-file-validate $(GENDIR)/$(APP_ID).desktop $(GENDIR)/$(APP_ID).autostart.desktop

# FORCE: the output depends on PREFIX as well as the template.
$(GENDIR)/%: data/%.in FORCE
	@mkdir -p $(@D)
	sed 's|@BINDIR@|$(BINDIR)|g' $< > $@

install: build $(GENERATED)
	install -Dm755 target/release/muse $(BINDIR)/muse
	install -Dm644 $(GENDIR)/$(APP_ID).desktop $(DATADIR)/applications/$(APP_ID).desktop
	install -Dm644 $(GENDIR)/$(APP_ID).service $(DATADIR)/dbus-1/services/$(APP_ID).service
	install -Dm644 data/icons/hicolor/symbolic/apps/$(APP_ID)-symbolic.svg \
		$(ICONDIR)/symbolic/apps/$(APP_ID)-symbolic.svg
	@# Prefer the official icon when `make fetch-icon` has downloaded it.
	if ls data/icons/official/*.png >/dev/null 2>&1; then \
		rm -f $(ICONDIR)/scalable/apps/$(APP_ID).svg; \
		for f in data/icons/official/*.png; do \
			size=$$(basename $$f .png); \
			install -Dm644 $$f $(ICONDIR)/$${size}x$${size}/apps/$(APP_ID).png; \
		done; \
	else \
		rm -f $(foreach s,$(OFFICIAL_SIZES),$(ICONDIR)/$(s)x$(s)/apps/$(APP_ID).png); \
		install -Dm644 data/icons/hicolor/scalable/apps/$(APP_ID).svg \
			$(ICONDIR)/scalable/apps/$(APP_ID).svg; \
	fi
	-update-desktop-database -q $(DATADIR)/applications
	-gtk4-update-icon-cache -q -t -f $(ICONDIR)
	@# The session bus reads service files at start and on request.
	-gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
		--method org.freedesktop.DBus.ReloadConfig >/dev/null
	@if [ -f $(AUTOSTARTDIR)/$(APP_ID).desktop ]; then \
		install -m644 $(GENDIR)/$(APP_ID).autostart.desktop $(AUTOSTARTDIR)/$(APP_ID).desktop; fi
	@echo "Installed. If Muse is running, quit it (Ctrl+Q) to start the new version."

uninstall: disable-autostart
	rm -f $(BINDIR)/muse
	rm -f $(DATADIR)/applications/$(APP_ID).desktop
	rm -f $(DATADIR)/dbus-1/services/$(APP_ID).service
	rm -f $(ICONDIR)/scalable/apps/$(APP_ID).svg $(ICONDIR)/symbolic/apps/$(APP_ID)-symbolic.svg
	rm -f $(foreach s,$(OFFICIAL_SIZES),$(ICONDIR)/$(s)x$(s)/apps/$(APP_ID).png)
	-update-desktop-database -q $(DATADIR)/applications
	-gtk4-update-icon-cache -q -t -f $(ICONDIR)
	@echo "Settings and login are kept in ~/.config/muse-client and ~/.local/share/muse-client."

enable-autostart: $(GENDIR)/$(APP_ID).autostart.desktop
	install -Dm644 $< $(AUTOSTARTDIR)/$(APP_ID).desktop

disable-autostart:
	rm -f $(AUTOSTARTDIR)/$(APP_ID).desktop

install-search-provider:
	sudo install -Dm644 data/$(APP_ID).search-provider.ini \
		$(SEARCH_PROVIDER_DIR)/$(APP_ID).search-provider.ini
	@echo "Log out and back in for GNOME Shell to pick it up."

uninstall-search-provider:
	sudo rm -f $(SEARCH_PROVIDER_DIR)/$(APP_ID).search-provider.ini

install-shortcut:
	scripts/install-shortcut.py '$(SHORTCUT)' '$(BINDIR)/muse --quick-ask'

uninstall-shortcut:
	scripts/install-shortcut.py --remove

fetch-icon:
	scripts/fetch-icon.sh $(OFFICIAL_SIZES)

FORCE:
