#!/bin/sh
# Test-only CAs for the SSH fixture, made on each machine so the repository holds no private key.
set -e
cd "$(dirname "$0")/ssh"
for ca in host-ca user-ca; do
    [ -f "$ca" ] || ssh-keygen -q -t ed25519 -N '' -C "ela test $ca" -f "$ca"
done
