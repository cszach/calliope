PREFIX ?= $(HOME)/.local
BINDIR := $(PREFIX)/bin
DATADIR := $(PREFIX)/share
ICONDIR := $(DATADIR)/icons/hicolor
AUTOSTARTDIR := $(or $(XDG_CONFIG_HOME),$(HOME)/.config)/autostart
# GNOME Shell reads search providers from system data dirs only.
SEARCH_PROVIDER_DIR := /usr/local/share/gnome-shell/search-providers
APP_ID := io.github.cszach.Muse
# The app id, D-Bus path and binary while the app was called Calliope;
# `make install` replaces such an install.
OLD_ID := io.github.cszach.Calliope
OLD_PATH := /io/github/cszach/Calliope
OLD_BIN := calliope

# Data files with the install path filled in.
GENDIR := target/data
GENERATED := $(GENDIR)/$(APP_ID).desktop $(GENDIR)/$(APP_ID).service \
	$(GENDIR)/$(APP_ID).autostart.desktop

# Fallback quick-ask key, for desktops without the GlobalShortcuts portal.
SHORTCUT ?= <Control><Alt>m

# Where scripts/fetch-icon.sh keeps Muse's icons; ignored by git.
OFFICIAL := data/icons/official
# Sizes of the official icon in both the muse.ai manifest and hicolor.
OFFICIAL_SIZES := 512 256 192 96 72 64 48 32
# The symbolic icon, which GNOME Shell draws in the top bar and in
# notification headers, plain and with a notification dot. With Muse's icons
# both are made at install time from the site's favicon.
SYMBOLIC_ICON := symbolic/apps/$(APP_ID)-symbolic.svg
SYMBOLIC_DOT_ICON := symbolic/apps/$(APP_ID)-attention-symbolic.svg
# The app icon `make install` puts under $(ICONDIR): Muse's, or the repository's.
OFFICIAL_ICONS := $(foreach s,$(OFFICIAL_SIZES),$(s)x$(s)/apps/$(APP_ID).png)
PLACEHOLDER_ICON := scalable/apps/$(APP_ID).svg

.PHONY: all build run check fmt clippy test validate bench install uninstall \
	install-shortcut uninstall-shortcut \
	install-search-provider uninstall-search-provider \
	fetch-icon FORCE

all: build

build:
	cargo build --release

run:
	cargo run -- --debug

check: fmt clippy test validate

# Measures memory, CPU and startup in a headless compositor (docs/performance.md).
# PROFILE: a directory with data/, config/ and cache/ XDG roots, e.g. a copy
# of a logged-in profile.
bench: build
	scripts/bench-headless $(if $(PROFILE),--profile $(PROFILE))

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
	@# Muse's own icons are Meta's artwork: they come from muse.ai, never from
	@# this repository. Without them (offline, say) the repository's stand in.
	@if scripts/fetch-icon.sh $(OFFICIAL_SIZES); then \
		set -e; \
		rm -f $(ICONDIR)/$(PLACEHOLDER_ICON); \
		for size in $(OFFICIAL_SIZES); do \
			install -Dm644 $(OFFICIAL)/$$size.png \
				$(ICONDIR)/$${size}x$${size}/apps/$(APP_ID).png; \
		done; \
		scripts/symbolic-icon.sh $(OFFICIAL)/mark.png > $(GENDIR)/symbolic.svg; \
		scripts/symbolic-icon.sh $(OFFICIAL)/mark.png --dot > $(GENDIR)/symbolic-dot.svg; \
		install -Dm644 $(GENDIR)/symbolic.svg $(ICONDIR)/$(SYMBOLIC_ICON); \
		install -Dm644 $(GENDIR)/symbolic-dot.svg $(ICONDIR)/$(SYMBOLIC_DOT_ICON); \
		echo "Installed Muse's icons."; \
	else \
		set -e; \
		echo "Could not download Muse's icons: installing placeholders." \
			"Run 'make install' again when online."; \
		rm -f $(addprefix $(ICONDIR)/,$(OFFICIAL_ICONS)); \
		for f in $(PLACEHOLDER_ICON) $(SYMBOLIC_ICON) $(SYMBOLIC_DOT_ICON); do \
			install -Dm644 data/icons/hicolor/$$f $(ICONDIR)/$$f; \
		done; \
	fi
	@# Replace Calliope: stop it if it is running (its login and settings move
	@# when Muse first starts), remove an install of it from this prefix, and
	@# keep autostart if it was on.
	-@if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
		--method org.freedesktop.DBus.NameHasOwner $(OLD_ID) 2>/dev/null | grep -q true; then \
		echo "Quitting Calliope"; \
		gdbus call --session --dest $(OLD_ID) --object-path $(OLD_PATH) \
			--method org.freedesktop.Application.ActivateAction quit '[]' '{}' >/dev/null; fi
	rm -f $(BINDIR)/$(OLD_BIN) $(DATADIR)/applications/$(OLD_ID).desktop \
		$(DATADIR)/dbus-1/services/$(OLD_ID).service \
		$(DATADIR)/metainfo/$(OLD_ID).metainfo.xml \
		$(ICONDIR)/scalable/apps/$(OLD_ID).svg $(ICONDIR)/symbolic/apps/$(OLD_ID)-symbolic.svg \
		$(ICONDIR)/symbolic/apps/$(OLD_ID)-attention-symbolic.svg \
		$(foreach s,$(OFFICIAL_SIZES),$(ICONDIR)/$(s)x$(s)/apps/$(OLD_ID).png)
	@if [ -f $(AUTOSTARTDIR)/$(OLD_ID).desktop ]; then \
		rm -f $(AUTOSTARTDIR)/$(OLD_ID).desktop; \
		install -Dm644 $(GENDIR)/$(APP_ID).autostart.desktop $(AUTOSTARTDIR)/$(APP_ID).desktop; fi
	-update-desktop-database -q $(DATADIR)/applications
	-gtk4-update-icon-cache -q -t -f $(ICONDIR)
	@# The session bus reads service files at start and on request.
	-gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
		--method org.freedesktop.DBus.ReloadConfig >/dev/null
	@if [ -f $(AUTOSTARTDIR)/$(APP_ID).desktop ]; then \
		install -m644 $(GENDIR)/$(APP_ID).autostart.desktop $(AUTOSTARTDIR)/$(APP_ID).desktop; fi
	@echo "Installed. If Muse is running, quit it (Ctrl+Q) to start the new version."

uninstall:
	rm -f $(AUTOSTARTDIR)/$(APP_ID).desktop
	rm -f $(BINDIR)/muse
	rm -f $(DATADIR)/applications/$(APP_ID).desktop
	rm -f $(DATADIR)/dbus-1/services/$(APP_ID).service
	rm -f $(addprefix $(ICONDIR)/,$(OFFICIAL_ICONS) $(PLACEHOLDER_ICON) \
		$(SYMBOLIC_ICON) $(SYMBOLIC_DOT_ICON))
	-update-desktop-database -q $(DATADIR)/applications
	-gtk4-update-icon-cache -q -t -f $(ICONDIR)
	@echo "Settings and login are kept in ~/.config/muse-gnome and ~/.local/share/muse-gnome."

install-search-provider:
	sudo rm -f $(SEARCH_PROVIDER_DIR)/$(OLD_ID).search-provider.ini
	sudo install -Dm644 data/$(APP_ID).search-provider.ini \
		$(SEARCH_PROVIDER_DIR)/$(APP_ID).search-provider.ini
	@echo "Log out and back in for GNOME Shell to pick it up."

uninstall-search-provider:
	sudo rm -f $(SEARCH_PROVIDER_DIR)/$(APP_ID).search-provider.ini

install-shortcut:
	scripts/install-shortcut.py '$(SHORTCUT)' '$(BINDIR)/muse --quick-ask'

uninstall-shortcut:
	scripts/install-shortcut.py --remove

# Downloads Muse's icon again, for when muse.ai has changed it.
fetch-icon:
	scripts/fetch-icon.sh --refresh $(OFFICIAL_SIZES)

FORCE:
