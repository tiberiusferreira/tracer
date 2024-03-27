use std::io::Read;

use reqwest::RequestBuilder;

use crate::print_if_dbg;

// reqwest doesn't support this out of the box: `https://github.com/seanmonstar/reqwest/issues/1217`
pub fn compress_and_set_body_and_with_encoding_headers(
    request_builder: RequestBuilder,
    body: &str,
) -> RequestBuilder {
    let context = "compressing request body";
    print_if_dbg(
        context,
        format!("Response before compression: {} bytes", body.len()),
    );
    let lg_window_size = 21;
    let quality = 4;
    let mut input =
        brotli::CompressorReader::new(body.as_bytes(), 4096, quality as u32, lg_window_size as u32);
    let mut compressed_body: Vec<u8> = Vec::with_capacity(100 * 1000);
    input.read_to_end(&mut compressed_body).unwrap();
    print_if_dbg(
        context,
        format!(
            "Response after compression: {} bytes",
            compressed_body.len()
        ),
    );
    request_builder
        .body(compressed_body)
        .header("Content-Encoding", "br")
}

#[cfg(test)]
mod test {
    #[test]
    fn set_compressed_body_and_set_encoding_headers_works() {
        let client = reqwest::Client::new();
        let post_request = client.post("https://test.com");
        let body = "Hello World.";
        let request_with_body =
            super::compress_and_set_body_and_with_encoding_headers(post_request, body)
                .build()
                .unwrap();
        let content_encoding = request_with_body.headers().get("Content-Encoding").unwrap();
        assert_eq!(content_encoding.to_str().unwrap(), "br");
        let mut body = request_with_body.body().unwrap().as_bytes().unwrap();
        let mut writer: Vec<u8> = Vec::new();
        brotli::BrotliDecompress(&mut body, &mut writer).unwrap();
        let decompressed_body = String::from_utf8(writer).unwrap();
        assert_eq!(decompressed_body, "Hello World.");
    }
}
