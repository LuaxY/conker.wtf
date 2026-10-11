#!/bin/sh
# certbot --deploy-hook (deploy/README.md): runs after each certificate is issued or renewed. Copies
# it where Caddy and conker-server (group conker) can read it, then restarts both: each reads its
# certificate at start (a reload with an unchanged Caddyfile may keep the old one), and the game
# server's matches in progress end (players reload the page).
set -e
install -d -m 0750 -o root -g conker /etc/conker/tls
install -m 0640 -o root -g conker "$RENEWED_LINEAGE/fullchain.pem" "$RENEWED_LINEAGE/privkey.pem" /etc/conker/tls/
systemctl restart caddy
systemctl restart conker-server
