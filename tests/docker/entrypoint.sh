#!/bin/sh
# A3SQL docker test entrypoint
unset ARMA3_SERVER__CDLC
# The a3sql extension .so must be in the server root for Arma to load it
cp /arma3/server/mods/@a3sql/a3sql_x64.so /arma3/server/a3sql_x64.so 2>/dev/null
# Symlink every mod into the game dir so the server resolves them
for m in /arma3/server/mods/@*; do
	[ -e "$m" ] || continue
	base=$(basename "$m")
	ln -sfn "$m" "/arma3/server/$base"
done
exec /usr/local/bin/arma3server
