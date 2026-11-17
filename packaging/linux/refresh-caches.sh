#!/bin/sh
# Run by the .deb and the .rpm after installing and after removing Arca.
#
# Rebuilds the caches the desktop reads the new desktop entry and icons from:
# without the first, "Open with" may not list Arca until the next login. On
# current distributions the triggers of desktop-file-utils and of the icon
# theme already do this, and running it again costs nothing. Each tool is
# optional, and a failure here must never fail the package transaction.
#
# There is no update-mime-database: every type Arca offers is already defined
# by shared-mime-info, so Arca ships no MIME definitions of its own.
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor || true
fi
exit 0
