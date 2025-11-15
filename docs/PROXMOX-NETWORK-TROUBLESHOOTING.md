# Proxmox Network Troubleshooting One‑Pager

## Summary
- **[issue]** Proxmox host UI/API became unreachable from Mac after home network changes.
- **[cause]** `vmbr0` stayed on old subnet `192.168.68.0/24` while the LAN moved to `192.168.0.0/24`. Hostname `nuc.pve.local` also resolved to old IP and `.local` conflicted with mDNS on macOS.
- **[fix]** Move Proxmox bridge to new subnet, update hostname resolution, use correct HTTPS endpoint and port 8006.

## Symptoms
- **[cli-fail]** Terraform/curl: no route to host or redirects to old IP.
- **[browser-mixed]** Safari sometimes loads; Chrome blocked.
- **[vm-visible]** VM (e.g., `vm101`) visible on `192.168.0.x` while host stayed on `192.168.68.x`.

## Root Cause
- **[static-ip-mismatch]** `vmbr0` configured static `192.168.68.69/24`, gateway `192.168.68.1`.
- **[hostname-redirect]** Proxmox redirects API/UI to node FQDN (e.g., `nuc.pve.local`) which still resolved to old IP.
- **[mdns]** `.local` domains use mDNS on macOS and can ignore `/etc/hosts`.

## Resolution (what worked)
1. **Add temporary IP on new LAN (non-destructive):**
   ```bash
   ip addr add 192.168.0.69/24 dev vmbr0
   ss -lntp | grep 8006
   systemctl restart pveproxy
   ```
2. **Make new IP permanent (pick UI or CLI):**
   - UI: `Datacenter -> <node> -> System -> Network -> vmbr0 -> Edit`
     - IPv4: Static `192.168.0.69/24`
     - Gateway: `192.168.0.1`
     - DNS: `192.168.0.1` (or preferred)
     - Apply (ifupdown2) or quick reboot.
   - CLI (`/etc/network/interfaces`):
     ```text
     auto vmbr0
     iface vmbr0 inet static
         address 192.168.0.69/24
         gateway 192.168.0.1
         bridge-ports <your-uplink-nic>
         bridge-stp off
         bridge-fd 0
     ```
     Apply: `ifreload -a` (ifupdown2) or `reboot`.
3. **Fix hostname resolution:**
   - Prefer a non-.local alias (e.g., `nuc.pve.lan`).
   - On Proxmox `/etc/hosts`:
     ```
     192.168.0.69  nuc.pve.local nuc
     192.168.0.69  nuc.pve.lan
     ```
     `systemctl restart pveproxy`
   - On macOS `/etc/hosts`:
     ```
     192.168.0.69  nuc.pve.lan
     ```
     Flush DNS: `sudo dscacheutil -flushcache && sudo killall -HUP mDNSResponder`
4. **Verify API:**
   ```bash
   # Expect 401 No ticket (means reachable)
   curl -vkL --http1.1 https://nuc.pve.lan:8006/api2/json/version
   ```
5. **Clean up old IP:**
   ```bash
   ip addr del 192.168.68.69/24 dev vmbr0  # if still present
   ip route
   ```

## Chrome/Safari Differences
- **[cert/HSTS]** Chrome is strict about self‑signed certs & HSTS.
- **[cleanup]** In Chrome: `chrome://net-internals/#hsts` → delete policies for the old IP/host and new host; clear site data; try Incognito.
- **[best-practice]** Regenerate a cert for your chosen hostname (`Datacenter -> <node> -> System -> Certificates`).

## API Auth Quick Reference
- **Unauthenticated GET** returns `401 No ticket` (normal).
- **Token auth (preferred):**
  ```bash
  curl -sk --http1.1 \
    -H 'Authorization: PVEAPIToken=USER@REALM!TOKENID=TOKENSECRET' \
    https://nuc.pve.lan:8006/api2/json/nodes
  ```
- **Ticket auth:**
  ```bash
  curl -sk --http1.1 -d "username=root@pam&password=PASS" \
    https://nuc.pve.lan:8006/api2/json/access/ticket
  ```

## Terraform Readiness
- **`.env` updates** in `infra-as-code/.env`:
  ```
  PROXMOX_ENDPOINT=https://nuc.pve.lan:8006/api2/json
  PROXMOX_USER=<user@realm>
  PROXMOX_TOKEN_ID=<user@realm!tokenid>
  PROXMOX_TOKEN_SECRET=<secret>
  PROXMOX_INSECURE=true   # until certs are trusted
  ```
- **Render & plan:**
  ```bash
  cd infra-as-code
  ./generate-config.sh
  make proxmox-terraform-plan
  # then: make proxmox-terraform-apply
  ```

## Prevention
- **Align host IPs with LAN changes:** update `vmbr0` or use DHCP reservation + DHCP on `vmbr0`.
- **Avoid `.local` for hostnames** on macOS; use `.lan` or your domain.
- **Keep a console fallback** (portable monitor or PiKVM/TinyPilot).
- **Certificates:** use a hostname and regenerate certs to match for smoother Chrome/curl behavior.
