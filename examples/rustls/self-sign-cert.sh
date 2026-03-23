#!/bin/sh
#
# Create a self-signed certificate for localhost that is valid for 1 week. This
# is not intended for production use, but is useful for testing.
#

set -ex

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CERT="$SCRIPT_DIR/localhost.cert"
KEY="$SCRIPT_DIR/localhost.key"

rm "$CERT" "$KEY" || true

# Generate private key (PKCS#8, DER)
openssl genpkey \
  -algorithm RSA \
  -pkeyopt rsa_keygen_bits:4096 \
  -outform DER \
  -out "$KEY"

# Generate self-signed certificate (DER)
openssl req \
  -new \
  -x509 \
  -key "$KEY" \
  -keyform DER \
  -outform DER \
  -out "$CERT" \
  -sha256 \
  -days 7 \
  -subj "/CN=localhost"
