#!/bin/sh
#
# Create a self-signed certificate for localhost and export it as a PKCS#12
# archive for use with the native-tls crate. Prompts for a password and writes
# it to a .env file.
#

# Remove any existing files.
rm -f localhost.cert localhost.key localhost.p12 .env

# Prompt the user for a password (no echo).
echo "Enter password for PKCS#12 file:"
stty -echo
read -r PASSWORD
stty echo
echo

set -e

# Generate a new private key and certificate.
openssl req \
    -x509 \
    -out localhost.cert \
    -keyout localhost.key \
    -newkey rsa:4096 \
    -nodes \
    -sha256 \
    -days 7 \
    -subj '/CN=localhost'

# Export to PKCS#12 format with the provided password.
openssl pkcs12 -export \
    -inkey localhost.key \
    -in localhost.cert \
    -out localhost.p12 \
    -password pass:"$PASSWORD"

# Clean up intermediate files.
rm -f localhost.cert localhost.key

# Write .env file.
echo "TLS_PKCS_PASSWORD=$PASSWORD" > .env

echo "PKCS#12 file generated as localhost.p12 and password saved in .env"
