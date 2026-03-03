# Examples

## hello

The hello world example from the [README](../README.md).

### Running the Example

```sh
cargo run --example hello
```

---

```sh
curl http://localhost:8080/hello/Via
# => Hello, Via!
```

## echo

The classic echo server example with a ws relay for GET requests.

### Running the Example

```sh
cargo run --features="ring tokio-tungstenite" --example echo
```

---

```sh
curl http:/localhost:8080/echo -d "Hello, Via!"
# => Hello, Via!
```

## native-tls

A native-tls version of the hello world example in the [README](../README.md).

### Setup

Run the following command to generate a self-signed .p12 cert and .env file.

```sh
./tls/self-sign-native.sh
```

### Running the Example

**Bash/Zsh/Sh**

```sh
set -a; . ./examples/tls/.env; set +a; \
  cargo run --example native-tls --features="native-tls"
```

**Fish**

```sh
env (cat ./examples/tls/.env | string split \n | string match -r '.*') \
  cargo run --example native-tls --features="native-tls"
```

---

```sh
curl -k https://localhost:8080/hello/Via
# => Hello, Via! (via TLS)
```

## rustls

A rustls version of the hello world example in the [README](../README.md).

### Setup

Run the following command to generate a self-signed cert and key.

```sh
./tls/self-sign-rustls.sh
```

### Running the Example

```sh
cargo run --example rustls --features="http2 ring rustls"
```

---

```sh
curl -k --http2-prior-knowledge https://localhost:8080/hello/Via
# => Hello, Via! (via TLS)
```

## cookies

A variation of the hello world example that keeps track of how many times you
have visited.

### Running the Example

```sh
cargo run --example cookies
```

---

```sh
curl -i http://localhost:8080/hello/Via
# => HTTP/1.1 200 OK
# => content-length: 11
# => content-type: text/plain; charset=utf-8
# => set-cookie: counter=KQ0oZDwlGHwMP1dFYswZ16h5WE1jzF3ko03iE3JCtbM=1; HttpOnly; SameSite=Strict; Secure; Path=/; Max-Age=86400
# => date: Tue, 03 Mar 2026 21:35:33 GMT
# =>
# => Hello, Via!⏎
```

## files

A simple file server with hardcoded mime type resolution.

### Running the Example

```sh
cargo run --example files
```

Visit http://localhost:8080/ in your browser to see a nostalgic web page with
an image of space on the about page.
