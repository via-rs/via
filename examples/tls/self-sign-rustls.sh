#!/bin/sh
#
# Create a self-signed certificate for localhost that is valid for 1 week. This
# is not intended for production use, but is useful for testing.
#

set -ex

# Remove any existing certificate and private key.
rm localhost.cert localhost.key || true

# Request a new certificate and private key.
openssl req \
    -x509 \
    -out localhost.cert \
    -keyout localhost.key \
    -newkey rsa:4096 \
    -nodes \
    -sha256 \
    -days 7 \
    -subj '/CN=localhost'
