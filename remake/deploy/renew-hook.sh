#!/bin/sh
# certbot --deploy-hook (deploy/README.md): runs after each certificate is issued or renewed. Copies
# it where Caddy and conker-server (group conker) can read it, then restarts Caddy (it reads its
# certificate at start; a reload with an unchanged Caddyfile may keep the old one). The game server
# is only started (the first issue; a no-op once it runs): it looks at the two files every 10 s and
# gives the new pair to new connections, so the matches in progress go on (`journalctl -u
# conker-server`: "certificate reloaded").
set -e
install -d -m 0750 -o root -g conker /etc/conker/tls
install -m 0640 -o root -g conker "$RENEWED_LINEAGE/fullchain.pem" "$RENEWED_LINEAGE/privkey.pem" /etc/conker/tls/
systemctl restart caddy
systemctl start conker-server
