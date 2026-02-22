use std::path::PathBuf;
use std::process::ExitCode;
use via::{Error, Next, Request, Server, response::File};

/// The maximum amount of memory that will be allocated to serve a single file.
const MAX_ALLOC_SIZE: usize = 1024 * 1024;
const PUBLIC_DIR: &str = "./examples/public";
const TEXT_HTML: &str = "text/html; charset=utf-8";

/// Serve the file at the provided path argument.
async fn serve(request: Request, _: Next) -> via::Result {
    let mut file_path = {
        let path = request
            .param("path")
            .decode()
            .optional()?
            .unwrap_or_else(|| "index.html".into());

        PathBuf::from(PUBLIC_DIR).join(path.as_ref())
    };

    let content_type = 'mime: {
        let Some(extension) = file_path.extension() else {
            file_path.set_extension("html");
            break 'mime TEXT_HTML;
        };

        match extension.to_str() {
            Some("html") => TEXT_HTML,
            Some("jpeg") => "image/jpeg",
            Some("css") => "text/css; charset=utf-8",
            Some("png") => "image/png",
            _ => "application/octet-stream",
        }
    };

    File::open(&file_path)
        .content_type(content_type)
        .with_last_modified()
        .serve(MAX_ALLOC_SIZE)
        .await
}

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let mut app = via::app(());

    // Serve any file located in the public dir.
    app.route("/*path").to(via::get(serve));

    Server::new(app).listen(("127.0.0.1", 8080)).await
}
