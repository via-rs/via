#!/bin/sh
#
# Create a self-signed certificate for localhost that is valid for 1 week. This
# is not intended for production use, but is useful for testing.
#

set -ex

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CERT="$SCRIPT_DIR/localhost.cert"
KEY="$SCRIPT_DIR/localhost.key"

# Remove any existing certificate and private key.
rm $CERT $KEY || true

# Request a new certificate and private key.
openssl req \
    -x509 \
    -out $CERT \
    -keyout $KEY \
    -newkey rsa:4096 \
    -nodes \
    -sha256 \
    -days 7 \
    -subj '/CN=localhost'
