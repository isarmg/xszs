//! The xcss-generated inventory and bytes are part of this executable.
include!(concat!(env!("OUT_DIR"), "/xcss-web-assets.rs"));

pub(crate) fn verify() -> anyhow::Result<()> {
    xcss::web_assets::verify_embedded(ASSETS, MANIFEST, DIGEST)
}

pub(crate) fn response(
    path: &str,
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
) -> axum::response::Response {
    xcss::web_assets::response(ASSETS, path, method, headers).map(axum::body::Body::from)
}
