#!/usr/bin/with-contenv bash
# The image turns forwarding off, and Jump Host tests need it on. The tests also open more
# connections at once than the default MaxStartups lets through before sign-in, and ask the shell
# which method signed in.
config=/config/sshd/sshd_config
sed -i 's/^AllowTcpForwarding no/AllowTcpForwarding yes/' "$config"
grep -q '^MaxStartups' "$config" || echo 'MaxStartups 100' >> "$config"
grep -q '^ExposeAuthInfo' "$config" || echo 'ExposeAuthInfo yes' >> "$config"
# A host certificate from the test-only CA in dev/ssh/host-ca, for the @cert-authority test.
keys=/config/ssh_host_keys
# Signed at every start, because the certificate outlives the CA in this volume.
if [ -r /ca/host-ca ]; then
    cp /ca/host-ca /tmp/host-ca && chmod 600 /tmp/host-ca
    ssh-keygen -q -s /tmp/host-ca -I ela-fixture -h -n localhost,127.0.0.1 "$keys/ssh_host_ed25519_key.pub"
    rm -f /tmp/host-ca
    chown --reference="$keys/ssh_host_ed25519_key" "$keys/ssh_host_ed25519_key-cert.pub"
fi
grep -q '^HostCertificate' "$config" || echo "HostCertificate $keys/ssh_host_ed25519_key-cert.pub" >> "$config"
# Keys signed by the test-only CA in dev/ssh/user-ca sign in, for the user certificate tests, and
# a banner shows before sign-in.
grep -q '^TrustedUserCAKeys' "$config" || echo 'TrustedUserCAKeys /ca/user-ca.pub' >> "$config"
echo 'Ela fixture banner: authorized use only.' > /config/banner.txt
grep -q '^Banner' "$config" || echo 'Banner /config/banner.txt' >> "$config"
# The tests fail sign-ins on purpose, and OpenSSH 9.8+ would then refuse the test host for a while.
grep -q '^PerSourcePenalties' "$config" || echo 'PerSourcePenalties no' >> "$config"
