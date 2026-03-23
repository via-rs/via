use std::path::PathBuf;
use via::{Middleware, Next, Request};

/// Serve the file at the provided path argument.
pub fn serve(public: PathBuf) -> impl Middleware<()> + 'static {
    use mime_guess::mime::TEXT_HTML_UTF_8;
    use via::response::File;

    move |request: Request, _: Next| {
        let public = public.clone();

        async move {
            let path = request.param("path").ok()?.unwrap_or("index.html".into());
            let mut path = public.join(path.as_ref());
            let content_type = if let Some(os_str) = path.extension()
                && let Some(ext) = os_str.to_str()
            {
                mime_guess::from_ext(ext).first_or_octet_stream()
            } else {
                path.set_extension("html");
                TEXT_HTML_UTF_8
            };

            File::open(&path)
                .content_type(&content_type)
                .with_last_modified()
                .serve(1024 * 1024) // stream files > 1 MB
                .await
        }
    }
}
