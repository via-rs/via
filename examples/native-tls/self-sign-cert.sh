#!/bin/sh
#
# Create a self-signed certificate for localhost and export it as a PKCS#12
# archive for use with the native-tls crate. Prompts for a password and writes
# it to a .env file.
#

set -e

# Resolve directory where this script lives
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

CERT="$SCRIPT_DIR/localhost.cert"
KEY="$SCRIPT_DIR/localhost.key"
P12="$SCRIPT_DIR/localhost.p12"
ENV_FILE="$SCRIPT_DIR/.env"

# Remove any existing files.
rm -f "$CERT" "$KEY" "$P12" "$ENV_FILE"

# Prompt the user for a password (no echo).
echo "Enter password for PKCS#12 file:"
stty -echo
read -r PASSWORD
stty echo
echo

# Generate a new private key and certificate.
openssl req \
    -x509 \
    -out "$CERT" \
    -keyout "$KEY" \
    -newkey rsa:4096 \
    -nodes \
    -sha256 \
    -days 7 \
    -subj '/CN=localhost'

# Export to PKCS#12 format with the provided password.
openssl pkcs12 -export \
    -inkey "$KEY" \
    -in "$CERT" \
    -out "$P12" \
    -password pass:"$PASSWORD"

# Clean up intermediate files.
rm -f "$CERT" "$KEY"

# Write .env file.
echo "TLS_PKCS_PASSWORD=$PASSWORD" > "$ENV_FILE"

echo "PKCS#12 file generated at: $P12"
echo "Password saved to: $ENV_FILE"