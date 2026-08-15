PREFIX ?= /usr
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share

all:
	cargo build --release

install:
	# Install binary
	install -Dm755 target/release/mercek $(DESTDIR)$(BINDIR)/mercek

	# Install desktop file
	install -Dm644 assets/dev.zuwu.Mercek.desktop \
		$(DESTDIR)$(DATADIR)/applications/dev.zuwu.Mercek.desktop

	# Install icons
	install -Dm644 assets/icons/128px.png \
		$(DESTDIR)$(DATADIR)/icons/hicolor/128x128/apps/dev.zuwu.Mercek.png
	install -Dm644 assets/icons/scalable.svg \
		$(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/dev.zuwu.Mercek.svg

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/mercek
	rm -f $(DESTDIR)$(DATADIR)/applications/dev.zuwu.Mercek.desktop
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/128x128/apps/dev.zuwu.Mercek.png
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/dev.zuwu.Mercek.svg